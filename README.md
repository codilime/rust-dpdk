# rust-dpdk

Tested on Ubuntu 18.04.5 LTS. There might be some problems with building on other distributions (like Centos or Arch).

## Instalation

First build and install DPDK (tested with DPDK 20.11 but should work with newer versions too):
```bash
git clone https://github.com/DPDK/dpdk.git
cd dpdk
git checkout v20.11

meson --buildtype=debug ../dpdk-build
ninja -C ../dpdk-build
export DPDK_INSTALL_PATH=`pwd`/../dpdk-install
DESTDIR=$DPDK_INSTALL_PATH meson install -C ../dpdk-build
```

now clone this repository and build and start examples:
```bash
cd offload-okr/rust-dpdk/apps
# DPDK_INSTALL_PATH must be set to start the building script.
# One can set it with the following command (if it's not set already):
# export DPDK_INSTALL_PATH=/path/to/installed/dpdk
cargo build
cargo run --example helloworld
cargo run --example l2fwd -- -- -p 3
```
