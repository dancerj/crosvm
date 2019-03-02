// Copyright 2018 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::os::unix::io::RawFd;

use audio_streams::StreamSource;
use pci::ac97_bus_master::Ac97BusMaster;
use pci::ac97_mixer::Ac97Mixer;
use pci::ac97_regs::*;
use pci::pci_configuration::{
    PciBarConfiguration, PciClassCode, PciConfiguration, PciHeaderType, PciMultimediaSubclass,
};
use pci::pci_device::{self, PciDevice, Result};
use pci::PciInterruptPin;
use resources::SystemAllocator;
use sys_util::{EventFd, GuestMemory};

// Use 82801AA because it's what qemu does.
const PCI_DEVICE_ID_INTEL_82801AA_5: u16 = 0x2415;

/// AC97 audio device emulation.
/// Provides the PCI interface for the internal Ac97 emulation.
/// Internally the `Ac97BusMaster` and `Ac97Mixer` structs are used to emulated the bus master and
/// mixer registers respectively. `Ac97BusMaster` handles moving smaples between guest memory and
/// the audio backend.
pub struct Ac97Dev {
    config_regs: PciConfiguration,
    // The irq events are temporarily saved here. They need to be passed to the device after the
    // jail forks. This happens when the bus is first written.
    irq_evt: Option<EventFd>,
    irq_resample_evt: Option<EventFd>,
    bus_master: Ac97BusMaster,
    mixer: Ac97Mixer,
}

impl Ac97Dev {
    /// Creates an 'Ac97Dev' that uses the given `GuestMemory` and starts with all registers at
    /// default values.
    pub fn new(mem: GuestMemory, audio_server: Box<StreamSource>) -> Self {
        let config_regs = PciConfiguration::new(
            0x8086,
            PCI_DEVICE_ID_INTEL_82801AA_5,
            PciClassCode::MultimediaController,
            &PciMultimediaSubclass::AudioDevice,
            None, // No Programming interface.
            PciHeaderType::Device,
            0x8086, // Subsystem Vendor ID
            0x1,    // Subsystem ID.
        );

        Ac97Dev {
            config_regs,
            irq_evt: None,
            irq_resample_evt: None,
            bus_master: Ac97BusMaster::new(mem, audio_server),
            mixer: Ac97Mixer::new(),
        }
    }

    fn read_mixer(&mut self, offset: u64, data: &mut [u8]) {
        match data.len() {
            // The mixer is only accessed with 16-bit words.
            2 => {
                let val: u16 = self.mixer.readw(offset);
                data[0] = val as u8;
                data[1] = (val >> 8) as u8;
            }
            l => error!("mixer read length of {}", l),
        }
    }

    fn write_mixer(&mut self, offset: u64, data: &[u8]) {
        match data.len() {
            // The mixer is only accessed with 16-bit words.
            2 => self
                .mixer
                .writew(offset, u16::from(data[0]) | u16::from(data[1]) << 8),
            l => error!("mixer write length of {}", l),
        }
        // Apply the new mixer settings to the bus master.
        self.bus_master.update_mixer_settings(&self.mixer);
    }

    fn read_bus_master(&mut self, offset: u64, data: &mut [u8]) {
        match data.len() {
            1 => data[0] = self.bus_master.readb(offset),
            2 => {
                let val: u16 = self.bus_master.readw(offset);
                data[0] = val as u8;
                data[1] = (val >> 8) as u8;
            }
            4 => {
                let val: u32 = self.bus_master.readl(offset);
                data[0] = val as u8;
                data[1] = (val >> 8) as u8;
                data[2] = (val >> 16) as u8;
                data[3] = (val >> 24) as u8;
            }
            l => error!("read length of {}", l),
        }
    }

    fn write_bus_master(&mut self, offset: u64, data: &[u8]) {
        match data.len() {
            1 => self.bus_master.writeb(offset, data[0], &self.mixer),
            2 => self
                .bus_master
                .writew(offset, u16::from(data[0]) | u16::from(data[1]) << 8),
            4 => self.bus_master.writel(
                offset,
                (u32::from(data[0]))
                    | (u32::from(data[1]) << 8)
                    | (u32::from(data[2]) << 16)
                    | (u32::from(data[3]) << 24),
            ),
            l => error!("write length of {}", l),
        }
    }
}

impl PciDevice for Ac97Dev {
    fn debug_label(&self) -> String {
        "AC97".to_owned()
    }

    fn assign_irq(
        &mut self,
        irq_evt: EventFd,
        irq_resample_evt: EventFd,
        irq_num: u32,
        irq_pin: PciInterruptPin,
    ) {
        self.config_regs.set_irq(irq_num as u8, irq_pin);
        self.irq_evt = Some(irq_evt);
        self.irq_resample_evt = Some(irq_resample_evt);
    }

    fn allocate_io_bars(&mut self, resources: &mut SystemAllocator) -> Result<Vec<(u64, u64)>> {
        let mut ranges = Vec::new();
        let mixer_regs_addr = resources
            .allocate_mmio_addresses(MIXER_REGS_SIZE)
            .ok_or(pci_device::Error::IoAllocationFailed(MIXER_REGS_SIZE))?;
        let mixer_config = PciBarConfiguration::default()
            .set_register_index(0)
            .set_address(mixer_regs_addr)
            .set_size(MIXER_REGS_SIZE);
        self.config_regs
            .add_pci_bar(&mixer_config)
            .ok_or_else(|| pci_device::Error::IoRegistrationFailed(mixer_regs_addr))?;
        ranges.push((mixer_regs_addr, MIXER_REGS_SIZE));

        let master_regs_addr = resources
            .allocate_mmio_addresses(MASTER_REGS_SIZE)
            .ok_or_else(|| pci_device::Error::IoAllocationFailed(MASTER_REGS_SIZE))?;
        let master_config = PciBarConfiguration::default()
            .set_register_index(1)
            .set_address(master_regs_addr)
            .set_size(MASTER_REGS_SIZE);
        self.config_regs
            .add_pci_bar(&master_config)
            .ok_or_else(|| pci_device::Error::IoRegistrationFailed(master_regs_addr))?;
        ranges.push((master_regs_addr, MASTER_REGS_SIZE));
        Ok(ranges)
    }

    fn config_registers(&self) -> &PciConfiguration {
        &self.config_regs
    }

    fn config_registers_mut(&mut self) -> &mut PciConfiguration {
        &mut self.config_regs
    }

    fn keep_fds(&self) -> Vec<RawFd> {
        if let Some(server_fds) = self.bus_master.keep_fds() {
            server_fds
        } else {
            Vec::new()
        }
    }

    fn read_bar(&mut self, addr: u64, data: &mut [u8]) {
        let bar0 = u64::from(self.config_regs.get_bar_addr(0));
        let bar1 = u64::from(self.config_regs.get_bar_addr(1));
        match addr {
            a if a >= bar0 && a < bar0 + MIXER_REGS_SIZE => self.read_mixer(addr - bar0, data),
            a if a >= bar1 && a < bar1 + MASTER_REGS_SIZE => {
                self.read_bus_master(addr - bar1, data)
            }
            _ => (),
        }
    }

    fn write_bar(&mut self, addr: u64, data: &[u8]) {
        let bar0 = u64::from(self.config_regs.get_bar_addr(0));
        let bar1 = u64::from(self.config_regs.get_bar_addr(1));
        match addr {
            a if a >= bar0 && a < bar0 + MIXER_REGS_SIZE => self.write_mixer(addr - bar0, data),
            a if a >= bar1 && a < bar1 + MASTER_REGS_SIZE => {
                // Check if the irq needs to be passed to the device.
                if let (Some(irq_evt), Some(irq_resample_evt)) =
                    (self.irq_evt.take(), self.irq_resample_evt.take())
                {
                    self.bus_master.set_irq_event_fd(irq_evt, irq_resample_evt);
                }
                self.write_bus_master(addr - bar1, data)
            }
            _ => (),
        }
    }
}
