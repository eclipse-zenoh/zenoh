# WARNING

This crate is intended for Zenoh's internal use.
It is not guaranteed that the API will remain unchanged in any version, including patch updates.
It is highly recommended to depend solely on the zenoh and zenoh-ext crates and to utilize their public APIs.

- [Click here for Zenoh's main repository](https://github.com/eclipse-zenoh/zenoh)
- [Click here for Zenoh's documentation](https://zenoh.io)

## The CAN link

A CAN FD transport, behind the `transport_can` feature. Linux only: it is built on SocketCAN.

**CAN FD is required, not preferred.** A classic CAN frame carries 8 bytes. A zenoh `Join` is 33 bytes at its smallest, and transport messages are never fragmented -- they must fit one batch whole or the session never opens. So a classic link is not a slower link, it is one that cannot establish at all. An interface that does not negotiate FD is refused at open, and `dbitrate=0` is refused at parse.

For classic CAN hardware, use the `isotp` link instead: ISO 15765-2 segments below zenoh, giving a 4095-byte MTU on 8-byte frames.

A CAN bus is a broadcast medium -- every node hears every frame and filters by identifier -- so this is a **multicast** link. Each peer owns one identifier, transmits on it, accepts frames from every other identifier the mask admits, and drops its own. The sender's identifier is that peer's address.

CAN frames are bounded and self-delimiting, so this is a **datagram** link: zenoh's transport fragments anything larger than the MTU, and the link itself needs no segmentation or reassembly.

### Endpoints

```text
can/<device>#bitrate=500000;dbitrate=2000000;id=0x100;match=0;mask=0
```

| key | meaning |
| --- | --- |
| `device` | the CAN interface name, such as `can0` or `vcan0` |
| `bitrate` | arbitration-phase bit rate |
| `dbitrate` | CAN FD data-phase bit rate. Must be non-zero |
| `id` | **this** peer's identifier, and its address on the bus |
| `match` | accept frames whose `(id & mask) == match` |
| `mask` | `0`, the default, accepts every identifier on the bus |
| `so_rcvbuf` | receive buffer in bytes. Absent, the kernel default applies |

On Linux the bit rates are advisory: rates are set out of band with `ip link set can0 type can bitrate ...`, and a virtual interface has none at all. They are still validated, because `dbitrate=0` used to select classic CAN and now means a misconfiguration.

The MTU is 63 bytes: one CAN FD frame, less a one-byte length prefix.

### `so_rcvbuf`, and when it matters

Frames arriving faster than the link drains them are dropped by the kernel,
silently, before the link sees them. A real bus cannot outrun the reader:
2 Mbit/s of CAN FD is under 2 800 frames per second. A **virtual** interface can,
because it has no bit rate at all.

Measured on `vcan0`: 100 messages of 4 KiB -- 71 frames each, 7 100 frames in a
burst -- lost 31% of messages on the default buffer, and none at all on 8 MiB.
Nothing reported an error in either case, which is what makes it worth naming.

The kernel clamps the request to `net.core.rmem_max` without saying so, so the
link reads the value back and warns when it fell short.

### Identifier value is bus priority

A **lower identifier wins arbitration**, so `id` is a real-time decision and not a name. The peer that must not be delayed needs the lower identifier.

The defaults are a starting point, not an allocation. Two peers that both accept them differ only by whichever was configured first, which is a priority ordering nobody chose. Priority is also per **peer**, not per message: one identifier carries all of a peer's traffic.

Only 11-bit identifiers are supported. `id`, `match` and `mask` above `0x7FF` are refused at open.

### Interoperating with zenoh-pico

zenoh-pico has a CAN link of the same wire format, and the two talk to each other
over one bus. One thing has to be arranged first.

zenoh-pico's `Z_BATCH_MULTICAST_SIZE` is a **compile-time constant, default
2048**, and it is advertised verbatim in the `Join` message no matter what the
link underneath can actually carry. Its receiver then rejects any peer whose
advertised batch size is not exactly equal to its own:

```c
/* zenoh-pico src/transport/multicast/rx.c */
if ((msg->_seq_num_res != Z_SN_RESOLUTION) || (msg->_req_id_res != Z_REQ_RESOLUTION) ||
    (msg->_batch_size != Z_BATCH_MULTICAST_SIZE)) {
    _Z_INFO("Couldn't accept peer because distant node is incompatible config wise.");
```

This link advertises `min(configured batch size, link MTU)`, which on CAN FD is
63. A stock zenoh-pico advertises 2048, so the two never associate -- and the
symptom is that single log line on the pico side and nothing at all on this one.

Build zenoh-pico with its multicast batch set to the CAN MTU:

```sh
cmake -S <zenoh-pico> -B build -DZ_FEATURE_LINK_CAN=1 -DBATCH_MULTICAST_SIZE=63
```

Then, with `vcan0` up and the two peers on different identifiers:

```sh
# zenoh-pico subscriber
build/examples/z_sub -m peer -k 'demo/example/**'     -l 'can/vcan0#bitrate=500000;dbitrate=2000000;id=0x200;match=0;mask=0'

# zenoh-rs publisher
cargo run -p zenoh-examples --features zenoh/transport_can --example z_pub --     -m peer -l 'can/vcan0#id=0x100' --no-multicast-scouting -k 'demo/example/from-rust'
```

Note the pico side must use `-l` (listen), never `-e` (connect): its CAN link
registers in `_z_listen_link` only, because a bus has no connection setup.

### Testing without hardware

The link runs against a virtual bus, which needs no CAN controller:

```sh
ci/vcan-setup.sh              # create and bring up vcan0, prompting for sudo
ci/vcan-setup.sh --status     # report, changing nothing
ci/vcan-setup.sh --down       # tear it down again
```

which is the equivalent of:

```sh
sudo modprobe vcan
sudo ip link add dev vcan0 type vcan
sudo ip link set up vcan0
```

`candump -td vcan0` then shows every frame. The end-to-end tests live in `io/zenoh-transport/tests/multicast_can.rs` and are `#[ignore]`d, so run them deliberately:

```sh
cargo test -p zenoh-transport --features transport_can --test multicast_can -- --ignored --nocapture
```
