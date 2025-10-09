# Zenoh Rust extra examples

## Start instructions

   When Zenoh is built in release mode:

   ```bash
   ./target/release/example/<example_name>
   ```

   Each example accepts the `-h` or `--help` option that provides a description of its arguments and their default values.

   If you run the tests against the Zenoh router running in a Docker container, you need to add the
   `-e tcp/localhost:7447` option to your examples. That's because Docker doesn't support UDP multicast
   transport, and therefore the Zenoh scouting and discovery mechanism cannot work with it.

## Examples description

### z_advanced_pub

   Declares an AdvancedPublisher with a given key expression.  
   All the publications are locally cached (with a configurable history size - i.e. max number of cached data per resource, default 1). The cache can be queried by an AdvancedSubscriber for history
   or retransmission.

   Typical usage:

   ```bash
   z_advanced_pub
   ```

   or

   ```bash
   z_advanced_pub --history 10
   ```

### z_advanced_sub

   Declares an AdvancedSubscriber with a given key expression.  
   The AdvancedSubscriber can query for AdvancedPublisher history at startup
   and on late joiner publisher detection. The AdvancedSubscriber can detect
   sample loss and ask for retransmission.

   Typical usage:

   ```bash
   z_advanced_sub
   ```

### z_member

   Group Management example: join a group and display the received group events (Join, Leave, LeaseExpired), as well as an updated group view.

   Typical usage:

   ```bash
   z_member
   ```

   (start/stop several in parallel)

### z_view_size

   Group Management example: join a group and wait for the group view to reach a configurable size (default: 3 members).

   Typical usage:

   ```bash
   z_view_size
   ```

   (start/stop several in parallel)
