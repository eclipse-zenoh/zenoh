# Zenoh-net Rust examples

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

### zn_scout

   Scouts for zenoh peers and routers available on the network.

   Typical usage:
   ```bash
      zn_scout
   ```

### zn_info

   Gets information about the zenoh-net session.

   Typical usage:
   ```bash
      zn_info
   ```


### zn_write

   Writes a path/value into Zenoh.  
   The path/value will be received by all matching subscribers, for instance the [zn_sub](#zn_sub)
   and [zn_storage](#zn_storage) examples.

   Typical usage:
   ```bash
      zn_write
   ```
   or
   ```bash
      zn_write -p /demo/example/test -v 'Hello World'
   ```

### zn_pub

   Declares a resource with a path and a publisher on this resource. Then writes a value using the numerical resource id.
   The path/value will be received by all matching subscribers, for instance the [zn_sub](#zn_sub)
   and [zn_storage](#zn_storage) examples.

   Typical usage:
   ```bash
      zn_pub
   ```
   or
   ```bash
      zn_pub -p /demo/example/test -v 'Hello World'
   ```

### zn_sub

   Registers a subscriber with a selector.  
   The subscriber will be notified of each write made on any path matching the selector,
   and will print this notification.

   Typical usage:
   ```bash
      zn_sub
   ```
   or
   ```bash
      zn_sub -s /demo/**
   ```

### zn_pull

   Registers a pull subscriber with a selector.  
   The pull subscriber will receive each write made on any path matching the selector,
   and will pull on demand and print the received path/value.

   Typical usage:
   ```bash
      zn_pull
   ```
   or
   ```bash
      zn_pull -s /demo/**
   ```

### zn_query

   Sends a query message for a selector.  
   The queryables with a matching path or selector (for instance [zn_eval](#zn_eval) and [zn_storage](#zn_storage))
   will receive this query and reply with paths/values that will be received by the query callback.

   Typical usage:
   ```bash
      zn_query
   ```
   or
   ```bash
      zn_query -s /demo/**
   ```

### zn_eval

   Registers a queryable function with a path.  
   This queryable function will be triggered by each call to a query operation on zenoh-net
   with a selector that matches the path, and will return a value to the querier.

   Typical usage:
   ```bash
      zn_eval
   ```
   or
   ```bash
      zn_eval -p /demo/example/eval -v 'This is the result'
   ```

### zn_storage

   Trivial implementation of a storage in memory.  
   This examples registers a subscriber and a queryable on the same selector.
   The subscriber callback will store the received paths/values in an hashmap.
   The queryable callback will answer to queries with the paths/values stored in the hashmap
   and that match the queried selector.

   Typical usage:
   ```bash
      zn_storage
   ```
   or
   ```bash
      zn_storage -s /demo/**
   ```

### zn_pub_thr & zn_sub_thr

   Pub/Sub throughput test.
   This example allows to perform throughput measurements between a pubisher performing
   write operations and a subscriber receiving notifications of those writes.

   Typical Subscriber usage:
   ```bash
      z_sub_thr
   ```

   Typical Publisher usage:
   ```bash
      z_pub_thr 1024
   ```