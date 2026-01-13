// HTTP Server for Nostr Mail Backend
// This allows the frontend to run in a browser and communicate with the backend via HTTP

use axum::{
    extract::State,
    http::Method,
    response::Json,
    routing::{get, post},
    Router,
};
use nostr_mail_lib::{AppState, init_app_state};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::cors::{CorsLayer, Any};

#[derive(Serialize, Deserialize)]
struct InvokeRequest {
    command: String,
    args: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct InvokeResponse {
    success: bool,
    data: Option<serde_json::Value>,
    error: Option<String>,
}

async fn invoke_handler(
    State(app_state): State<Arc<AppState>>,
    Json(request): Json<InvokeRequest>,
) -> Json<InvokeResponse> {
    println!("[HTTP] Received command: {}", request.command);
    
    // Route commands to appropriate handlers
    let result = match request.command.as_str() {
        "greet" => {
            let name = request.args.get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("World");
            Ok(serde_json::json!(format!("Hello, {}! You've been greeted from Rust!", name)))
        },
        "generate_keypair" => {
            match nostr_mail_lib::generate_keypair_http() {
                Ok(kp) => Ok(serde_json::to_value(kp).unwrap()),
                Err(e) => Err(e),
            }
        },
        "validate_private_key" => {
            let private_key = match request.args.get("privateKey")
                .and_then(|v| v.as_str()) {
                Some(pk) => pk,
                None => {
                    return Json(InvokeResponse {
                        success: false,
                        data: None,
                        error: Some("Missing privateKey parameter".to_string()),
                    });
                }
            };
            match nostr_mail_lib::validate_private_key_http(private_key) {
                Ok(valid) => Ok(serde_json::json!(valid)),
                Err(e) => Err(e),
            }
        },
        "validate_public_key" => {
            let public_key = match request.args.get("publicKey")
                .and_then(|v| v.as_str()) {
                Some(pk) => pk,
                None => {
                    return Json(InvokeResponse {
                        success: false,
                        data: None,
                        error: Some("Missing publicKey parameter".to_string()),
                    });
                }
            };
            match nostr_mail_lib::validate_public_key_http(public_key) {
                Ok(valid) => Ok(serde_json::json!(valid)),
                Err(e) => Err(e),
            }
        },
        "get_public_key_from_private" => {
            let private_key = match request.args.get("privateKey")
                .and_then(|v| v.as_str()) {
                Some(pk) => pk,
                None => {
                    return Json(InvokeResponse {
                        success: false,
                        data: None,
                        error: Some("Missing privateKey parameter".to_string()),
                    });
                }
            };
            match nostr_mail_lib::get_public_key_from_private_http(private_key) {
                Ok(pubkey) => Ok(serde_json::json!(pubkey)),
                Err(e) => Err(e),
            }
        },
        "init_database" => {
            match nostr_mail_lib::http_init_database(app_state.clone()).await {
                Ok(_) => Ok(serde_json::json!({ "success": true })),
                Err(e) => Err(e),
            }
        },
        "db_get_all_contacts" => {
            let user_pubkey = match request.args.get("userPubkey")
                .and_then(|v| v.as_str()) {
                Some(pk) => pk.to_string(),
                None => {
                    return Json(InvokeResponse {
                        success: false,
                        data: None,
                        error: Some("Missing userPubkey parameter".to_string()),
                    });
                }
            };
            match nostr_mail_lib::http_db_get_all_contacts(user_pubkey, app_state.clone()).await {
                Ok(contacts) => Ok(serde_json::to_value(contacts).unwrap()),
                Err(e) => Err(e),
            }
        },
        "db_get_all_relays" => {
            match nostr_mail_lib::http_db_get_all_relays(app_state.clone()).await {
                Ok(relays) => Ok(serde_json::to_value(relays).unwrap()),
                Err(e) => Err(e),
            }
        },
        "db_get_emails" => {
            let limit = request.args.get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(50) as usize;
            let offset = request.args.get("offset")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let nostr_only = request.args.get("nostrOnly")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let user_email = request.args.get("userEmail")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            
            match nostr_mail_lib::http_db_get_emails(
                app_state.clone(),
                limit,
                offset,
                nostr_only,
                user_email,
            ).await {
                Ok(emails) => Ok(serde_json::to_value(emails).unwrap()),
                Err(e) => Err(e),
            }
        },
        "get_relays" => {
            match nostr_mail_lib::http_get_relays(app_state.clone()).await {
                Ok(relays) => Ok(serde_json::to_value(relays).unwrap()),
                Err(e) => Err(e),
            }
        },
        "set_relays" => {
            let relays: Vec<nostr_mail_lib::Relay> = match serde_json::from_value(
                request.args.get("relays")
                    .cloned()
                    .unwrap_or(serde_json::json!([]))
            ) {
                Ok(r) => r,
                Err(e) => {
                    return Json(InvokeResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid relays format: {}", e)),
                    });
                }
            };
            
            match nostr_mail_lib::http_set_relays(app_state.clone(), relays).await {
                Ok(_) => Ok(serde_json::json!({ "success": true })),
                Err(e) => Err(e),
            }
        },
        "init_persistent_nostr_client" => {
            let private_key = match request.args.get("privateKey")
                .and_then(|v| v.as_str()) {
                Some(pk) => pk.to_string(),
                None => {
                    return Json(InvokeResponse {
                        success: false,
                        data: None,
                        error: Some("Missing privateKey parameter".to_string()),
                    });
                }
            };
            
            match nostr_mail_lib::http_init_persistent_nostr_client(app_state.clone(), private_key).await {
                Ok(_) => Ok(serde_json::json!({ "success": true })),
                Err(e) => Err(e),
            }
        },
        "sync_relay_states" => {
            match nostr_mail_lib::http_sync_relay_states(app_state.clone()).await {
                Ok(updated_relays) => Ok(serde_json::to_value(updated_relays).unwrap()),
                Err(e) => Err(e),
            }
        },
        "get_relay_status" => {
            match nostr_mail_lib::http_get_relay_status(app_state.clone()).await {
                Ok(statuses) => Ok(serde_json::to_value(statuses).unwrap()),
                Err(e) => Err(e),
            }
        },
        "start_live_event_subscription" => {
            let private_key = match request.args.get("privateKey")
                .and_then(|v| v.as_str()) {
                Some(pk) => pk.to_string(),
                None => {
                    return Json(InvokeResponse {
                        success: false,
                        data: None,
                        error: Some("Missing privateKey parameter".to_string()),
                    });
                }
            };
            
            match nostr_mail_lib::http_start_live_event_subscription(app_state.clone(), private_key).await {
                Ok(_) => Ok(serde_json::json!({ "success": true })),
                Err(e) => Err(e),
            }
        },
        _ => Err(format!("Unknown command: {}", request.command)),
    };
    
    match result {
        Ok(data) => Json(InvokeResponse {
            success: true,
            data: Some(data),
            error: None,
        }),
        Err(e) => Json(InvokeResponse {
            success: false,
            data: None,
            error: Some(e),
        }),
    }
}

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

#[tokio::main]
async fn main() {
    println!("[HTTP] Starting Nostr Mail HTTP Server...");
    
    // Initialize app state
    let app_state = init_app_state().await;
    let app_state = Arc::new(app_state);
    
    // Build CORS layer
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);
    
    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/invoke", post(invoke_handler))
        .layer(ServiceBuilder::new().layer(cors))
        .with_state(app_state);
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:1420")
        .await
        .expect("Failed to bind to port 1420");
    
    println!("[HTTP] Server listening on http://127.0.0.1:1420");
    println!("[HTTP] Health check: http://127.0.0.1:1420/health");
    println!("[HTTP] Invoke endpoint: http://127.0.0.1:1420/invoke");
    
    axum::serve(listener, app)
        .await
        .expect("Server failed");
}
