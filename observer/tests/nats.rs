#![cfg(feature = "nats_integration_tests")]

use observer::infra::config::NatsConfig;
use observer::infra::nats::prepare_connection;
use shared::testing::nats_server::NatsServerForTesting;

#[tokio::test]
async fn natsutil_user_password_incorrect_is_rejected() {
    let user = "michael";
    let pass = "scott";
    let server = NatsServerForTesting::new(&["--user", user, "--pass", pass]).await;
    let address = format!("127.0.0.1:{}", server.port);

    let result = prepare_connection(&NatsConfig {
        address: address.clone(),
        username: Some(user.to_string()),
        password: Some("incorrect".to_string()),
    })
    .unwrap()
    .connect(address)
    .await;

    match result {
        Err(err) => assert!(
            matches!(
                err.kind(),
                async_nats::ConnectErrorKind::AuthorizationViolation
            ),
            "unexpected error kind: {err:?}"
        ),
        Ok(_) => panic!("expected authorization error, but connection succeeded"),
    }
}

#[tokio::test]
async fn natsutil_user_password_correct_connects() {
    let user = "michael";
    let pass = "scott";
    let server = NatsServerForTesting::new(&["--user", user, "--pass", pass]).await;
    let address = format!("127.0.0.1:{}", server.port);

    prepare_connection(&NatsConfig {
        address: address.clone(),
        username: Some(user.to_string()),
        password: Some(pass.to_string()),
    })
    .expect("preparing the connection should succeed")
    .connect(address)
    .await
    .expect("using the correct user/password should connect");
}

#[tokio::test]
async fn natsutil_no_auth_connects() {
    let server = NatsServerForTesting::new(&[]).await;
    let address = format!("127.0.0.1:{}", server.port);

    prepare_connection(&NatsConfig {
        address: address.clone(),
        username: None,
        password: None,
    })
    .expect("preparing the connection should succeed")
    .connect(address)
    .await
    .expect("connecting without auth should succeed");
}
