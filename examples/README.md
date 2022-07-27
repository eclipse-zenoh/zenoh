# Zenoh Rust examples

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

### z_scout

   Scouts for zenoh peers and routers available on the network.

   Typical usage:
   ```bash
      z_scout
   ```

### z_info

   Gets information about the zenoh session.

   Typical usage:
   ```bash
      z_info
   ```


### z_put

   Puts a path/value into Zenoh.  
   The path/value will be received by all matching subscribers, for instance the [z_sub](#z_sub)
   and [z_storage](#z_storage) examples.

   Typical usage:
   ```bash
      z_put
   ```
   or
   ```bash
      z_put -k /demo/example/test -v 'Hello World'
   ```

### z_pub

   Declares a key expression and a publisher. Then writes values periodically on the declared key expression.
   The published value will be received by all matching subscribers, for instance the [z_sub](#z_sub) and [z_storage](#z_storage) examples.

   Typical usage:
   ```bash
      z_pub
   ```
   or
   ```bash
      z_pub -k /demo/example/test -v 'Hello World'
   ```

### z_sub

   Declares a key expression and a susbcriber.  
   The subscriber will be notified of each `put` or `delete` made on any key expression matching the subscriber key expression, and will print this notification.

   Typical usage:
   ```bash
      z_sub
   ```
   or
   ```bash
      z_sub -k /demo/**
   ```

### z_pull

   Declares a key expression and a pull susbcriber.  
   On each pull, the pull subsciber will be notified of the last `put` or `delete` made on each key expression matching the subscriber key expression, and will print this notification.


   Typical usage:
   ```bash
      z_pull
   ```
   or
   ```bash
      z_pull -k /demo/**
   ```

### z_get

   Sends a query message for a selector.  
   The queryables with a matching path or selector (for instance [z_eval](#z_eval) and [z_storage](#z_storage))
   will receive this query and reply with paths/values that will be received by the receiver stream.

   Typical usage:
   ```bash
      z_get
   ```
   or
   ```bash
      z_get -k /demo/**
   ```

### z_eval

   Declares a queryable function with a path.  
   This queryable function will be triggered by each call to get
   with a selector that matches the path, and will return a value to the querier.

   Typical usage:
   ```bash
      z_eval
   ```
   or
   ```bash
      z_eval -k /demo/example/eval -v 'This is the result'
   ```

### z_storage

   Trivial implementation of a storage in memory.  
   This examples declares a subscriber and a queryable on the same selector.
   The subscriber callback will store the received paths/values in an hashmap.
   The queryable callback will answer to queries with the paths/values stored in the hashmap
   and that match the queried selector.

   Typical usage:
   ```bash
      z_storage
   ```
   or
   ```bash
      z_storage -k /demo/**
   ```

### z_pub_shm & z_sub_shm

   A pub/sub example involving the shared-memory feature.

   Typical Subscriber usage:
   ```bash
      z_sub_shm
   ```

   Typical Publisher usage:
   ```bash
      z_pub_shm
   ```

### z_pub_thr & z_sub_thr

   Pub/Sub throughput test.
   This example allows to perform throughput measurements between a pubisher performing
   put operations and a subscriber receiving notifications of those puts.

   Typical Subscriber usage:
   ```bash
      z_sub_thr
   ```

   Typical Publisher usage:
   ```bash
      z_pub_thr 1024
   ```

### z_ping & z_pong

   Pub/Sub roundtrip time test.
   This example allows to perform roundtrip time measurements. The z_ping example 
   performs a put operation on a first key expression, waits for a reply from the pong 
   example on a second key expression and measures the time between the two.
   The pong application waits for samples on the first key expression and replies by
   writing back the received data on the second key expression.

   Typical Pong usage:
   ```bash
      z_pong
   ```

   Typical Ping usage:
   ```bash
      z_ping 1024
   ```

### z_pub_shm_thr & z_sub_shm_thr

   Pub/Sub throughput test involving the shared-memory feature.
   This example allows to perform throughput measurements between a pubisher performing
   put operations with the shared-memory feature and a subscriber receiving notifications
   of those puts.

   Typical Subscriber usage:
   ```bash
      z_sub_shm_thr
   ```

   Typical Publisher usage:
   ```bash
      z_pub_shm_thr
   ```
