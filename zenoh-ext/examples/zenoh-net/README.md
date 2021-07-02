# Zenoh-net Rust extra examples

## Start instructions

   When zenoh is built in release mode:
   ```bash
   ./target/release/example/<example_name>
   ```

   Each example accepts the `-h` or `--help` option that provides a description of its arguments and their default values.

   If you run the tests against the zenoh router running in a Docker container, you need to add the
   `-e tcp/localhost:7447` option to your examples. That's because Docker doesn't support UDP multicast
   transport, and therefore the zenoh scouting and discrovery mechanism cannot work with.

## Examples description

### zn pub_cache

   Declares a resource with a path and a publisher plus a publication cache on this resource.  
   All the publications are locally cached (with a configurable history size - i.e. max number of cached data per resource). The cache can be queried by a QueryingSubscriber at startup (see next example).

   Typical usage:
   ```bash
      zn_pub_cache
   ```
   or
   ```bash
      zn_pub_cache --history 10
   ```

### zn_query_sub

   Registers a querying subscriber with a selector.  
   At startup, the subscriber issuez a query (by default on the same selector than the subscription) and merge/sort/de-duplicate the query results with the publications received in parallel.

   Typical usage:
   ```bash
      zn_query_sub
   ```


### zn_member

   Group Management example: join a group and display the received group events (Join, Leave, LeaseExpired), as well as an updated group view.

   Typical usage:
   ```bash
      zn_member
   ```
   (start/stop several in parallel)

### zn_view_size

   Group Management example: join a group and wait for the group view to reach a configurable size (default: 3 members).

   Typical usage:
   ```bash
      zn_view_size
   ```
   (start/stop several in parallel)

