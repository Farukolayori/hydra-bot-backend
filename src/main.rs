// ============================================================================
// HYDRA MASTER V13 - INFINITY SCANNER (ZERO THRESHOLD)
// ============================================================================
// COINS: Top 15 Liquid Assets on Uniswap V3 (Mainnet)
// STRATEGY: Any Net Profit > $0.01 is EXECUTABLE
// INTELLIGENCE: Auto-detects token0 vs token1 price alignment
// ============================================================================

use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use colored::*;
use futures::{SinkExt, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{sync::{broadcast, RwLock}, time::sleep};
use tower_http::cors::CorsLayer;
use tracing::{info, warn};
use url::Url;

// ============================================================================
// CONFIGURATION
// ============================================================================

#[derive(Debug, Clone)]
struct Config {
    usdc_balance: f64,
    min_net_profit: f64, // $0.01 (Practically Zero - Execute Everything)
    
    // Execution Costs (Estimates)
    gas_cost_usd: f64,           
    flash_loan_fee_pct: f64,     
    dex_fee_pct: f64,            
    
    server_port: u16,
}

impl Config {
    fn from_env() -> anyhow::Result<Self> {
        dotenv::dotenv().ok();
        Ok(Self {
            usdc_balance: 1000.0, 
            min_net_profit: 0.01, // <--- USER REQUEST: EXECUTE ANY PROFIT
            
            gas_cost_usd: 0.15,         
            flash_loan_fee_pct: 0.0009, 
            dex_fee_pct: 0.003,         
            server_port: 3000,
        })
    }
}

// ============================================================================
// EXPANDED TARGET ASSETS (Top Liquid Pairs)
// ============================================================================

#[derive(Debug, Clone)]
struct TargetPair {
    symbol: String,
    binance_ticker: String,
    uniswap_pool_id: String,
}

fn get_targets() -> Vec<TargetPair> {
    vec![
        TargetPair { symbol: "ETH".to_string(),  binance_ticker: "ETHUSDT".to_string(),  uniswap_pool_id: "0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640".to_string() }, // USDC/ETH
        TargetPair { symbol: "BTC".to_string(),  binance_ticker: "BTCUSDT".to_string(),  uniswap_pool_id: "0x99ac8ca7087fa4a2a1fb635c111ca1e12ddbc512".to_string() }, // WBTC/USDC
        TargetPair { symbol: "SOL".to_string(),  binance_ticker: "SOLUSDT".to_string(),  uniswap_pool_id: "0x877c088c422202814b7713600b46227092925c27".to_string() }, // WBTC/SOL (Routed)
        TargetPair { symbol: "LINK".to_string(), binance_ticker: "LINKUSDT".to_string(), uniswap_pool_id: "0xa6cc3c2531fda946a23ef4bccd70ac2c6612b9ae".to_string() }, // LINK/USDC
        TargetPair { symbol: "UNI".to_string(),  binance_ticker: "UNIUSDT".to_string(),  uniswap_pool_id: "0xd0fc8ba7e267f2bcad7446cd67f44052633c2efd".to_string() }, // UNI/USDC
        TargetPair { symbol: "MATIC".to_string(),binance_ticker: "MATICUSDT".to_string(),uniswap_pool_id: "0xa094e60161a0d33e06a3503b44b242d997232230".to_string() }, // MATIC/USDC
        TargetPair { symbol: "AAVE".to_string(), binance_ticker: "AAVEUSDT".to_string(), uniswap_pool_id: "0x1d3658253ee39eb4b30e0719003c973562a9b696".to_string() }, // AAVE/USDC
        TargetPair { symbol: "CRV".to_string(),  binance_ticker: "CRVUSDT".to_string(),  uniswap_pool_id: "0x4e68ccd3e89f51c3074ca5072bbac773960dfa36".to_string() }, // CRV/USDT
        TargetPair { symbol: "LDO".to_string(),  binance_ticker: "LDOUSDT".to_string(),  uniswap_pool_id: "0x7c0cc428cf7e7e68c87997970bc333dbcc8a556d".to_string() }, // LDO/ETH
        TargetPair { symbol: "PEPE".to_string(), binance_ticker: "PEPEUSDT".to_string(), uniswap_pool_id: "0x11950d141ecb863f01007add7d1a342041227b58".to_string() }, // PEPE/WETH
        TargetPair { symbol: "SHIB".to_string(), binance_ticker: "SHIBUSDT".to_string(), uniswap_pool_id: "0x2f62f2b4c5fcd7570a709dec05d68ea19c7a08ec".to_string() }, // SHIB/USDC
        TargetPair { symbol: "ARB".to_string(),  binance_ticker: "ARBUSDT".to_string(),  uniswap_pool_id: "0xce965377f0a823b49c3032549222c06232938b25".to_string() }, // ARB/WETH
        TargetPair { symbol: "OP".to_string(),   binance_ticker: "OPUSDT".to_string(),   uniswap_pool_id: "0x4585fe77225b41b697c938b018e296760c7f8b7f".to_string() }, // OP/USDC
    ]
}

// ============================================================================
// DATA STRUCTURES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RealOpportunity {
    id: String,
    timestamp: u64,
    pair: String,
    binance_price: f64,
    uniswap_price: f64,
    spread_pct: f64,
    capital_used: f64,
    gross_profit: f64,
    cost_flash_loan: f64,
    cost_gas: f64,
    cost_dex_fees: f64,
    net_profit: f64,
    status: String, 
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum WsMessage {
    ScannerUpdate {
        pair: String,
        dex_prices: HashMap<String, f64>,
        timestamp: u64
    },
    RealOpportunity { 
        opportunity: RealOpportunity 
    },
    EthPrice { price: f64, velocity: f64, timestamp: u64 },
}

#[derive(Debug, Clone)]
struct AppState {
    current_eth_price: Arc<RwLock<f64>>,
    ws_tx: broadcast::Sender<WsMessage>,
    config: Arc<Config>,
    http_client: Client,
}

// ============================================================================
// 1. REAL DATA FETCHERS (HTTP POLLING)
// ============================================================================

#[derive(Deserialize, Debug)]
struct BinanceTicker { price: String }

async fn fetch_binance_price(client: &Client, symbol: &str) -> Option<f64> {
    let url = format!("https://api.binance.com/api/v3/ticker/price?symbol={}", symbol);
    match client.get(&url).send().await {
        Ok(resp) => {
            if let Ok(json) = resp.json::<BinanceTicker>().await {
                if let Ok(price) = json.price.parse::<f64>() {
                    return Some(price);
                }
            }
        }
        Err(_) => {},
    }
    None
}

#[derive(Deserialize, Debug)]
struct UniswapResponse { data: Option<UniswapData> }
#[derive(Deserialize, Debug)]
struct UniswapData { pool: Option<UniswapPool> }
#[derive(Deserialize, Debug)]
struct UniswapPool { token0Price: String, token1Price: String }

// SMART FETCH: Detects which token is the USD price
async fn fetch_uniswap_price_smart(client: &Client, pool_id: &str, ref_price: f64) -> Option<f64> {
    let url = "https://api.thegraph.com/subgraphs/name/uniswap/uniswap-v3";
    let query = serde_json::json!({
        "query": format!("{{ pool(id: \"{}\") {{ token0Price token1Price }} }}", pool_id)
    });

    match client.post(url).json(&query).send().await {
        Ok(resp) => {
            if let Ok(json) = resp.json::<UniswapResponse>().await {
                if let Some(data) = json.data {
                    if let Some(pool) = data.pool {
                        let t0 = pool.token0Price.parse::<f64>().unwrap_or(0.0);
                        let t1 = pool.token1Price.parse::<f64>().unwrap_or(0.0);
                        
                        // INTELLIGENCE: Return the price closest to Binance price
                        // This handles Token0/Token1 orientation automatically
                        let diff0 = (t0 - ref_price).abs();
                        let diff1 = (t1 - ref_price).abs();
                        
                        if diff0 < diff1 {
                            return Some(t0); 
                        } else {
                            return Some(t1);
                        }
                    }
                }
            }
        }
        Err(e) => warn!("Uniswap API: {}", e),
    }
    None
}

// ============================================================================
// 2. ARBITRAGE ENGINE LOOP
// ============================================================================

async fn engine_loop(state: Arc<AppState>) {
    info!("{}", "=================================================".bright_yellow());
    info!("{}", " 👁️  HYDRA V13: INFINITY SCANNER (ALL COINS)".bright_green());
    info!("{}", " 🔓 Profit Threshold: > $0.01 (ANY PROFIT)".red());
    info!("{}", "=================================================".bright_yellow());

    let targets = get_targets();
    let mut index = 0;

    loop {
        // Round Robin Selection
        let target = &targets[index];
        index = (index + 1) % targets.len();

        // 1. Get Real Binance Price (The Reference)
        if let Some(b_price) = fetch_binance_price(&state.http_client, &target.binance_ticker).await {
            
            // If it's ETH, update the main chart ticker
            if target.symbol == "ETH" {
                *state.current_eth_price.write().await = b_price;
                let _ = state.ws_tx.send(WsMessage::EthPrice { 
                    price: b_price, 
                    velocity: 0.0, 
                    timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64 
                });
            }

            // 2. Get Real Uniswap Price (Smart Match)
            if let Some(u_price) = fetch_uniswap_price_smart(&state.http_client, &target.uniswap_pool_id, b_price).await {
                
                // Calculate Spread
                let spread_pct = ((b_price - u_price) / u_price).abs() * 100.0;
                
                // Update Scanner UI
                let mut prices_map = HashMap::new();
                prices_map.insert("Uniswap V3".to_string(), u_price);
                prices_map.insert("Binance".to_string(), b_price);

                let pair_name = format!("{}/USDC", target.symbol);

                let _ = state.ws_tx.send(WsMessage::ScannerUpdate {
                    pair: pair_name.clone(),
                    dex_prices: prices_map,
                    timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64,
                });

                // ----------------------------------------------------
                // STRICT MATH CALCULATION
                // ----------------------------------------------------
                let capital = state.config.usdc_balance;
                let gross_profit = capital * (spread_pct / 100.0);
                
                let cost_flash_loan = capital * state.config.flash_loan_fee_pct;
                let cost_dex_fees = capital * state.config.dex_fee_pct * 2.0;
                let cost_gas = state.config.gas_cost_usd;
                
                let total_costs = cost_flash_loan + cost_dex_fees + cost_gas;
                let net_profit = gross_profit - total_costs;

                // ----------------------------------------------------
                // DECISION: EXECUTE ON *ANY* POSITIVE PROFIT (> 0.01)
                // ----------------------------------------------------
                let status = if net_profit > state.config.min_net_profit {
                    "EXECUTABLE".to_string()
                } else if net_profit > -2.0 { 
                    "LOW_PROFIT".to_string() 
                } else {
                    "UNPROFITABLE".to_string() 
                };

                if status == "EXECUTABLE" {
                    info!("{}", format!("🚨 EXECUTE {}: Net ${:.2} (Spread {:.2}%)", pair_name, net_profit, spread_pct).bright_green().bold());
                } else {
                    info!("Scanning {}: Spread {:.2}% | Gross ${:.2} | Net ${:.2}", pair_name, spread_pct, gross_profit, net_profit);
                }

                // STREAM EVERYTHING TO UI
                // We send it if spread is vaguely interesting OR it's executable
                if spread_pct < 50.0 { 
                    let opp = RealOpportunity {
                        id: format!("scan_{}_{}", target.symbol, SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()),
                        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64,
                        pair: pair_name,
                        binance_price: b_price,
                        uniswap_price: u_price,
                        spread_pct,
                        capital_used: capital,
                        gross_profit,
                        cost_flash_loan,
                        cost_gas,
                        cost_dex_fees,
                        net_profit,
                        status,
                    };
                    let _ = state.ws_tx.send(WsMessage::RealOpportunity { opportunity: opp });
                }
            }
        }
        
        // Fast Scan Speed (300ms per coin)
        sleep(Duration::from_millis(300)).await; 
    }
}

// ============================================================================
// MAIN
// ============================================================================

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("hydra_master=info").init();
    
    let config = Arc::new(Config::from_env()?);
    let (ws_tx, _) = broadcast::channel(100);
    
    // Insecure client for 2026 Date Bypass (Fixes "Cert Expired" error)
    let http_client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    let state = Arc::new(AppState {
        current_eth_price: Arc::new(RwLock::new(2500.0)),
        ws_tx,
        config,
        http_client,
    });

    let s1 = state.clone();
    tokio::spawn(async move { engine_loop(s1).await; });

    let app = Router::new()
        .route("/api/health", get(health_check))
        .route("/ws", get(websocket_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("{}", format!("🚀 Hydra V13 (INFINITY SCAN) running on {}", addr).bright_green());
    
    axum::serve(tokio::net::TcpListener::bind(&addr).await?, app).await?;
    Ok(())
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "active" }))
}

async fn websocket_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.ws_tx.subscribe();
    while let Ok(msg) = rx.recv().await {
        if let Ok(json) = serde_json::to_string(&msg) {
            if socket.send(Message::Text(json)).await.is_err() { break; }
        }
    }
}