# Deb container

Experimental set up to build crosvm in pristine Debian environment,
for Debian package acceptance.

```shell
P=$(readlink -f $(pwd)/../../..)
podman build -t debcrosvm .
podman run -it --rm -v $P:$P:rw -w $(pwd) debcrosvm

```


## missing in Debian


```shell
cargo install cargo-debstatus
/root/.cargo/bin/cargo-debstatus debstatus > debstatus-output.txt


grep -v 'in debian' debstatus-output.txt  | awk '{print  $(NF-1) " " $NF}' | grep -v ')' | sort | uniq -c | sort -rn
   2185 remain v0.2.3
    438 uuid v0.8.2
    195 heck v0.3.3
    195 argh_derive v0.1.8
    195 argh v0.1.8
     80 memoffset v0.5.6
     80 intrusive-collections v0.9.4
     25 enumn v0.1.4
      1 terminal_size v0.1.17
      1 document-features v0.2.6

    438 uuid v0.8.2
    271 │ [dev-dependencies]
     37 │ [build-dependencies]
      1 terminal_size v0.1.17
      1 document-features v0.2.6
      1 [dev-dependencies] [dev-dependencies]
      1 [build-dependencies] [build-dependencies]

```

- intrusive-collections v0.9.4
- memoffset v0.5.6
- remain v0.2.3
- uuid v0.8.2
- argh v0.1.8
- argh_derive v0.1.8
- heck v0.3.3
- enumn v0.1.4



- remain v0.2.3
- uuid v0.8.2 -- 1.2.1 is packaged
- heck v0.3.3 -- 0.4.0 is packaged
- argh_derive v0.1.8
- argh v0.1.8
- memoffset v0.5.6 -- 0.6.5 is packaged, can we use that instead?
- intrusive-collections v0.9.4
- enumn v0.1.4
- terminal_size v0.1.17 -- 0.2.1 is packaged.
- document-features v0.2.6
