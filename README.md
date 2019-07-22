# jocker
Docker-like container thingy, made as an experiment on Linux namespaces and isolation.

### Build

```
> cargo build
```

### Install

```
> cargo install --path .
```

### Example

```
> sudo jocker image build -t some_image some_image_directory/
...
> sudo jocker run some_image /bin/bash
Creating container with ID 71ed76cf-9297-4e54-bf3f-9fbd3f49b6d7 from image some_image
Running container with ID 71ed76cf-9297-4e54-bf3f-9fbd3f49b6d7
root@71ed76cf-9297-4e54-bf3f-9fbd3f49b6d7:/# ls
bin  boot  dev	etc  home  lib	lib64  media  mnt  opt	proc  root  run  sbin  srv  sys  tmp  usr  var
root@71ed76cf-9297-4e54-bf3f-9fbd3f49b6d7:/# exit
>
```

### Usage

```
USAGE:
    jocker <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    container    Manage existing containers
    help         Prints this message or the help of the given subcommand(s)
    image        Manage images
    run          Create and run containers
```
