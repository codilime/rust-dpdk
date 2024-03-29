# rust-dpdk

Rust is a programming language designed for performance and safety, especially safe concurrency. Its syntax is similar to C++, but it can guarantee memory safety using a borrow checker to validate references.

DPDK (Data Plane Development Kit) is a set of libraries for implementing user space drivers for NICs (Network Interface Controllers). It provides a set of abstractions which allows a sophisticated packet processing pipeline to be programmed. DPDK allows for high performance while programming networking applications.
DPDK is written in C, so using it in Rust is inconvenient and not safe without a properly prepared API. Therefore, we decided to create Rust bindings to DPDK.

We are not the first ones who attempted it. We decided to base our API on some other project — https://github.com/ANLAB-KAIST/rust-dpdk. This project uses bindgen while compiling a code to generate bindings to the specified DPDK version. Thanks to that, it's not hard to update the API to the newer DPDK version. Additionally, a good deal of the high-level API was already well written so we didn't need to write it from scratch. Ultimately, we only added a few features to this library and fixed some issues.

The interface for communication with DPDK has been designed in such a way that the programmer doesn't have to remember not obvious dependencies that could often cause errors in DPDK applications. Check [l2fwd sources](l2fwd/src/main.rs) for reference.

## Environment setup

Tested on Ubuntu 18.04.5 LTS. There might be some problems with building on other distributions (like Centos or Arch).

Below are instructions on how to prepare a VM with two interfaces passed to DPDK application. These steps are not required if you want to start it with a different environment.

Prepare VM images
``` bash
wget -c https://cloud-images.ubuntu.com/bionic/current/bionic-server-cloudimg-amd64.img

cat > user-data <<EOF
#cloud-config
password: ubuntu
chpasswd: { expire: False }
ssh_pwauth: True
EOF

cloud-localds user-data.img user-data
```

Create interfaces for sending and receiving traffic
```bash
sudo brctl addbr br0
sudo ip tuntap add dev tap0 mode tap
sudo brctl addif br0 tap0
sudo ifconfig br0 10.30.0.1/24
sudo ip r add 10.30.0.2/32 via 10.30.0.1
sudo ifconfig br0 up
sudo ifconfig tap0 up

sudo brctl addbr br1
sudo ip tuntap add dev tap1 mode tap
sudo brctl addif br1 tap1
sudo ifconfig br1 10.31.0.1/24
sudo ip r add 10.31.0.2/32 via 10.31.0.1
sudo ifconfig br1 up
sudo ifconfig tap1 up
```

`ip a` result should look similar to this:
```
4: br0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue state UP group default qlen 1000
    link/ether 76:b4:22:27:f2:b2 brd ff:ff:ff:ff:ff:ff
    inet 10.30.0.1/24 brd 10.30.0.255 scope global br0
       valid_lft forever preferred_lft forever
    inet6 fe80::74b4:22ff:fe27:f2b2/64 scope link
       valid_lft forever preferred_lft forever
5: tap0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc fq_codel master br0 state UP group default qlen 1000
    link/ether c6:df:d6:3b:7c:94 brd ff:ff:ff:ff:ff:ff
    inet6 fe80::c4df:d6ff:fe3b:7c94/64 scope link
       valid_lft forever preferred_lft forever
6: br1: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue state UP group default qlen 1000
    link/ether 86:6c:4c:45:6b:91 brd ff:ff:ff:ff:ff:ff
    inet 10.31.0.1/24 brd 10.31.0.255 scope global br1
       valid_lft forever preferred_lft forever
    inet6 fe80::846c:4cff:fe45:6b91/64 scope link
       valid_lft forever preferred_lft forever
7: tap1: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc fq_codel master br1 state UP group default qlen 1000
    link/ether 06:3d:56:b3:d9:c1 brd ff:ff:ff:ff:ff:ff
    inet6 fe80::43d:56ff:feb3:d9c1/64 scope link
       valid_lft forever preferred_lft forever
```

Start VM
```bash
qemu-system-x86_64 \
        -cpu host \
        -enable-kvm \
        -drive file=bionic-server-cloudimg-amd64.img,format=qcow2 \
        -drive file=user-data.img,format=raw \
        -m 8192 \
        -smp 8 \
        -nographic \
        -net nic,model=virtio \
        -net user,hostfwd=tcp::2222-:22 \
        -netdev tap,id=mynet0,ifname=tap0,script=no,downscript=no \
        -device virtio-net,netdev=mynet0,mac=52:55:00:d1:55:01 \
        -netdev tap,id=mynet1,ifname=tap1,script=no,downscript=no \
        -device virtio-net,netdev=mynet1,mac=52:55:00:d1:55:02
```

`lspci` on VM should output two additional network devices that will be used to in DPDK apps
```
00:04.0 Ethernet controller: Red Hat, Inc. Virtio network device
00:05.0 Ethernet controller: Red Hat, Inc. Virtio network device
```

Clone DPDK
```
git clone https://github.com/DPDK/dpdk.git /path/to/dpdk
```

Bind these interfaces to DPDK compatible driver (like igb_uio)
```bash
# build igb_uio driver or install it with apt-get install dpdk-igb-uio-dkms
modprobe igb_uio
/path/to/dpdk/usertools/dpdk-devbind.py -b igb_uio 00:04.0 00:05.0
```

Also, Rust should be available - https://www.rust-lang.org/tools/install.

## Building and starting examples

First, build and install DPDK (tested with DPDK 20.11 but should work with newer versions too):
```bash
cd /path/to/dpdk/dpdk
git checkout v20.11

meson --buildtype=debug ../dpdk-build
ninja -C ../dpdk-build
export DPDK_INSTALL_PATH=`pwd`/../dpdk-install
DESTDIR=$DPDK_INSTALL_PATH meson install -C ../dpdk-build
```

now clone this repository, build and start examples:
```bash
cd offload-okr/rust-dpdk/apps
# DPDK_INSTALL_PATH must be set to start the building script.
# One can set it with the following command (if it's not set already):
# export DPDK_INSTALL_PATH=/path/to/installed/dpdk
cargo run --release --bin l2fwd
```

## Licensing

This project is licensed under the [BSD 3-Clause License](LICENSE). Please see the [LICENSE](LICENSE) file for more details.

We also include the [rust-dpdk](https://github.com/ANLAB-KAIST/rust-dpdk) project in the `binding` directory, which is also licensed under the [BSD 3-Clause License](binding/LICENSE). Please note that we have made modifications to the `ANLAB-KAIST/rust-dpdk` project to suit our needs.

If you have any questions or need further details, feel free to contact us.

Thank you for using our repository!
