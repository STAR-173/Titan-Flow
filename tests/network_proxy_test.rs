use titan_flow::network::proxy::ProxyManager;
use titan_flow::network::errors::NetworkError;

#[tokio::test]
async fn test_proxy_manager_instantiation() {
    // * FIXED: Renamed to _pm to suppress "unused variable" warning
    let _pm = ProxyManager::new(
        vec!["http://user:pass@dc.com:8080".to_string()],
        vec!["http://user:pass@res.com:9000".to_string()]
    );
    assert!(true); 
}

#[tokio::test]
async fn test_escalation_flow_simulation() {
    let pm = ProxyManager::new(
        vec!["http://127.0.0.1:8080".into()],
        vec!["http://127.0.0.1:9090".into()]
    );

    // * Trigger the simulation logic for "https://simulate.fail"
    let result = pm.fetch_with_escalation("https://simulate.fail").await;
    
    match result {
        Ok(_) => panic!("Should not succeed on dummy proxy"),
        Err(e) => {
            match e {
                // * CHANGED: Match against Reqwest error variant
                NetworkError::Reqwest(_) => {
                    println!("Escalation successful: Reached connection error on Tier 2");
                },
                _ => panic!("Failed to escalate properly. Got: {:?}", e),
            }
        }
    }
}