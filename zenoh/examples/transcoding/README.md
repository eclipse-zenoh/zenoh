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

### z_put

   Puts a path/value into Zenoh.  
   The path/value will be stored by all the storages with a selector that matches the path.
   It will also be received by all the matching subscribers (see [z_sub](#z_sub) below).  
   
   :warning:  **Note that if no storage and no subscriber are matching the path, the path/value will be dropped.**
   Therefore, you probably want to start zenohd with at least a memory storage (--mem-storage option) and/or start [z_sub](#z_sub) before z_put.

   Typical usage:
   ```bash
      z_put
   ```
   or
   ```bash
      z_put -p /demo/example/test -v 'Hello World'
   ```

### z_put_float

   Puts a path/value into Zenoh where the value is a float.
   The path/value will be stored by all the storages with a selector that matches the path.
   It will also be received by all the matching subscribers (see [z_sub](#z_sub) below).
   
   :warning: **Note that if no storage and no subscriber are matching the path, the path/value will be dropped.**
   Therefore, you probably want to start zenohd with at least a memory storage (--mem-storage option) and/or start [z_sub](#z_sub) before z_put.

   Typical usage:
   ```bash
      z_put_float
   ```
   or
   ```bash
      z_put_float -p /demo/example/test -v 1.61803398875
   ```

### z_get

   Gets a list of paths/values from Zenoh.  
   The values will be retrieved from the Storages containing paths that match the specified selector.  
   The Eval functions (see [z_eval](#z_eval) below) registered with a path matching the selector
   will also be triggered.

   Typical usage:
   ```bash
      z_get
   ```
   or
   ```bash
      z_get -s /demo/**
   ```

### z_delete

   Deletes a path and its associated value from Zenoh.  
   Any storage that store the path/value will drop it.  
   The subscribers with a selector matching the path will also receive a notification of this deletion.

   Typical usage:
   ```bash
      z_delete
   ```
   or
   ```bash
      z_delete -p /demo/example/test
   ```

### z_sub

   Registers a subscriber with a selector.  
   The subscriber will be notified of each put/remove made on any path matching the selector,
   and will print this notification.

   Typical usage:
   ```bash
      z_sub
   ```
   or
   ```bash
      z_sub -s /demo/**
   ```

### z_eval

   Registers an evaluation function with a path.  
   This evaluation function will be triggered by each call to a get operation on Zenoh 
   with a selector that matches the path. In this example, the function returns a string value.
   See the code for more details.

   Typical usage:
   ```bash
      z_eval
   ```
   or
   ```bash
      z_eval -p /demo/example/eval
   ```

### z_put_thr & z_sub_thr

   Pub/Sub throughput test.
   This example allows to perform throughput measurements between a pubisher performing
   put operations and a subscriber receiving notifications of those put.
   Note that you can run this example with or without any storage.

   Typical Subscriber usage:
   ```bash
      z_sub_thr
   ```

   Typical Publisher usage:
   ```bash
      z_put_thr 1024
   ```
