use std::time::Duration;

use async_std::task;
use zenoh::{publication::Priority, SessionDeclarations};
use zenoh_core::{zasync_executor_init, AsyncResolve};
use zenoh_protocol::core::CongestionControl;

const SLEEP: Duration = Duration::from_secs(1);

#[test]
fn pubsub() {
    task::block_on(async {
        zasync_executor_init!();
        let session1 = zenoh::open(zenoh_config::peer()).res_async().await.unwrap();
        let session2 = zenoh::open(zenoh_config::peer()).res_async().await.unwrap();

        let publisher = session1
            .declare_publisher("test/qos")
            .res()
            .await
            .unwrap()
            .priority(Priority::DataHigh)
            .congestion_control(CongestionControl::Drop);

        task::sleep(SLEEP).await;

        let sub = session2.declare_subscriber("test/qos").res().await.unwrap();
        task::sleep(SLEEP).await;

        publisher.put("qos").res_async().await.unwrap();
        let qos = sub.recv_async().await.unwrap().qos;

        assert_eq!(qos.priority, Priority::DataHigh.into());
        assert_eq!(qos.congestion_control, CongestionControl::Drop);

        let publisher = publisher
            .priority(Priority::DataLow)
            .congestion_control(CongestionControl::Block);
        publisher.put("qos").res_async().await.unwrap();
        let qos = sub.recv_async().await.unwrap().qos;

        assert_eq!(qos.priority, Priority::DataLow.into());
        assert_eq!(qos.congestion_control, CongestionControl::Block);
    });
}
