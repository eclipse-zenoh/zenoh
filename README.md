![zenoh banner](http://zenoh.io/img/zenoh-dragon-small.png)

[![CI](https://github.com/eclipse-zenoh/zenoh/workflows/CI/badge.svg)](https://github.com/eclipse-zenoh/zenoh/actions?query=workflow%3A%22CI%22)
[![Documentation Status](https://readthedocs.org/projects/zenoh-rust/badge/?version=latest)](https://zenoh-rust.readthedocs.io/en/latest/?badge=latest)
[![Gitter](https://badges.gitter.im/atolab/zenoh.svg)](https://gitter.im/atolab/zenoh?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)
[![License](https://img.shields.io/badge/License-EPL%202.0-blue)](https://choosealicense.com/licenses/epl-2.0/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# Eclipse zenoh
The Eclipse zenoh: Zero Overhead Pub/sub, Store/Query and Compute.

Eclipse zenoh /zeno/ unifies data in motion, data in-use, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) for more detailed information.

-------------------------------
## How to build it

Install [Cargo and Rust](https://doc.rust-lang.org/cargo/getting-started/installation.html). 
Zenoh can be succesfully compiled with Rust stable (>= 1.5.1), so no special configuraiton is required from your side. 
To build zenoh, just type the following command after having followed the previous instructions:

```bash
$ cargo build --release --all-targets
```

-------------------------------
## How to test it

For convenience, the zenoh router is pre-build and made available in a Docker image: https://hub.docker.com/r/eclipse/zenoh

Thus, run it just doing:
```bash
docker pull eclipse/zenoh:latest
docker run --init -p 7447:7447/tcp -p 7447:7447/udp -p 8000:8000/tcp eclipse/zenoh:latest
```

The ports used by zenoh are the following:

  - **7447/tcp** : the zenoh protocol via TCP
  - **7447/udp** : the zenoh scouting protocol using UDP multicast (for clients to automatically discover the router)
  - **8000/tcp** : the zenoh REST API


All the examples are compiled into the `target/release/examples` directory. They can all work in peer-to-peer, or interconnected via the zenoh router (`target/release/zenohd`).

Then, you can test it using the zenoh API in your favorite language:

 - **Rust** using the [zenoh crate](https://crates.io/crates/zenoh) and the [examples in this repo](https://github.com/eclipse-zenoh/zenoh/tree/master/zenoh/examples)
 - **Python** using [zenoh-python](https://github.com/eclipse-zenoh/zenoh-python)

Or with the **REST** API:

## Examples of usage with the REST API

The complete Eclipse zenoh's key/value space is accessible through the REST API, using regular HTTP GET, PUT and DELETE methods. In those examples, we use the **curl** command line tool.

### Managing the admin space

 * Get info of the local zenoh router:
   ```
   curl http://localhost:8000/@/router/local
   ```
 * Get the backends of the local router (only memory by default):
   ```
   curl 'http://localhost:8000/@/router/local/**/backend/*'
   ```
 * Get the storages of the local router (none by default):
   ```
   curl 'http://localhost:8000/@/router/local/**/storage/*'
   ```
 * Add a memory storage on `/demo/example/**`:
   ```
   curl -X PUT -H 'content-type:application/properties' -d 'path_expr=/demo/example/**' http://localhost:8000/@/router/local/plugin/storages/backend/memory/storage/my-storage

   ```

### Put/Get into zenoh
Assuming the memory storage has been added, as described above, you can now:

 * Put a key/value into zenoh:
  ```
  curl -X PUT -d 'Hello World!' http://localhost:8000/demo/example/test
  ```
 * Retrieve the key/value:
  ```
  curl http://localhost:8000/demo/example/test
  ```
 * Remove the key value
  ```
  curl -X DELETE http://localhost:8000/demo/example/test
  ```

