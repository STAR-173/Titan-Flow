use titan_flow::network::client::FastClient;

#[tokio::test]
async fn test_client_initialization() {
    let client = FastClient::new(None);
    assert!(client.is_ok());
}

#[tokio::test]
async fn test_fetch_google_safe() {
    let client = FastClient::new(None).unwrap();
    let result = client.fetch("https://www.google.com").await;
    
    match result {
        Ok(body) => assert!(body.len() > 100),
        Err(e) => panic!("Connection failed: {:?}", e),
    }
}
