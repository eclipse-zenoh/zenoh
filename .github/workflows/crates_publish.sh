cargo login $1
(cd commons/zenoh-result && cargo publish; cargo clean)
(cd commons/zenoh-core && cargo publish; cargo clean)
(cd commons/zenoh-keyexpr && cargo publish; cargo clean)
(cd commons/zenoh-collections && cargo publish; cargo clean)
(cd commons/zenoh-crypto && cargo publish; cargo clean)
(cd commons/zenoh-buffers && cargo publish; cargo clean)
(cd commons/zenoh-protocol && cargo publish; cargo clean)
(cd commons/zenoh-util && cargo publish; cargo clean)
(cd commons/zenoh-sync && cargo publish; cargo clean)
(cd commons/zenoh-macros && cargo publish; cargo clean)
(cd commons/zenoh-shm && cargo publish; cargo clean)
(cd commons/zenoh-codec && cargo publish; cargo clean)
(cd commons/zenoh-config && cargo publish; cargo clean)
(cd io/zenoh-link-commons && cargo publish; cargo clean)
(cd io/zenoh-links/zenoh-link-udp && cargo publish; cargo clean)
(cd io/zenoh-links/zenoh-link-tcp && cargo publish; cargo clean)
(cd io/zenoh-links/zenoh-link-tls && cargo publish; cargo clean)
(cd io/zenoh-links/zenoh-link-quic && cargo publish; cargo clean)
(cd io/zenoh-links/zenoh-link-unixpipe && cargo publish; cargo clean)
(cd io/zenoh-links/zenoh-link-unixsock_stream && cargo publish; cargo clean)
(cd io/zenoh-links/zenoh-link-serial && cargo publish; cargo clean)
(cd io/zenoh-links/zenoh-link-ws && cargo publish; cargo clean)
(cd io/zenoh-link && cargo publish; cargo clean)
(cd io/zenoh-transport && cargo publish; cargo clean)
(cd plugins/zenoh-plugin-trait && cargo publish; cargo clean)
(cd zenoh && cargo publish; cargo clean)
(cd zenoh-ext && cargo publish; cargo clean)
(cd zenohd && cargo publish; cargo clean)
(cd plugins/zenoh-plugin-rest && cargo publish; cargo clean)
(cd plugins/zenoh-backend-traits && cargo publish; cargo clean)
(cd plugins/zenoh-plugin-storage-manager && cargo publish; cargo clean)