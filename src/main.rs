use std::env;

use client::NewOrderRequest;
use dotenv::dotenv;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

mod client;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    dotenv().ok();
    let access_key = env::var("ACCESS_KEY").expect("ACCESS_KEY must be set");
    let api_secret_key = env::var("API_SECRET_KEY").expect("API_SECRET_KEY must be set");
    let access_window_time: u16 = env::var("ACCESS_WINDOW_TIME")
        .unwrap_or("5000".to_string())
        .parse()
        .unwrap();
    let asset = env::var("ASSET").unwrap_or("btc".to_string());
    let client = client::BitbankClient::new(&access_key, &api_secret_key, access_window_time);
    println!("{:?}", client);
    let pair = &format!("{}_jpy", &asset);
    let min_amount_for_sell: f32 = env::var("MIN_AMOUNT_FOR_SELL")
        .unwrap_or("1.0".to_string())
        .parse()
        .unwrap();
    let trigger_rate: f32 = env::var("TRIGGER_RATE")
        .unwrap_or("0.5".to_string())
        .parse()
        .unwrap();
    loop {
        let assets = client.get_assets().await.unwrap();
        let (amount, side) = match assets.get_asset(&asset) {
            Some(asset) => {
                let free_amount: f32 = asset.free_amount.parse().unwrap();
                if free_amount < min_amount_for_sell {
                    let jpy = assets.get_asset("jpy").unwrap();
                    let price = jpy.free_amount.parse::<f32>().unwrap() * 0.7;
                    (price, "buy")
                } else {
                    (free_amount, "sell")
                }
            }
            None => {
                continue;
            }
        };
        let (ws_stream, _) =
            connect_async("wss://stream.bitbank.cc/socket.io/?EIO=4&transport=websocket")
                .await
                .unwrap();
        let (mut ws_write, mut ws_read) = ws_stream.split();
        let message = Message::Text(r#"40"#.into());
        ws_write.send(message).await.unwrap();
        let message = Message::Text(format!(r#"42["join-room","ticker_{}"]"#, pair).into());
        ws_write.send(message).await.unwrap();
        while let Some(msg) = ws_read.next().await {
            let msg = msg.unwrap();
            if msg.is_text() {
                let text = msg.to_text().unwrap();
                if text.starts_with(r#"42["message",{"room_name":"ticker_"#) {
                    let v: serde_json::Value = serde_json::from_str(&text[2..]).unwrap();
                    let data = &v[1]["message"]["data"];
                    let sell = data["sell"].as_str().unwrap().parse::<f32>().unwrap();
                    let buy = data["buy"].as_str().unwrap().parse::<f32>().unwrap();
                    let high = data["high"].as_str().unwrap().parse::<f32>().unwrap();
                    let low = data["low"].as_str().unwrap().parse::<f32>().unwrap();
                    let middle = (high + low) / 2.0;
                    let sell_trigger_price = middle * (1.0 - trigger_rate) + high * trigger_rate;
                    let buy_trigger_price = middle * (1.0 - trigger_rate) + low * trigger_rate;
                    println!(
                        "sell: {:.2}, buy: {:.2}, high: {:.2}, low: {:.2}, sell_trigger_price: {:.2}, buy_trigger_price: {:.2}",
                        sell, buy, high, low, sell_trigger_price, buy_trigger_price
                    );
                    let req = if side == "sell" && sell >= sell_trigger_price {
                        Some(NewOrderRequest::new(pair, &format!("{amount:.6}"), side))
                    } else if side == "buy" && buy <= buy_trigger_price {
                        let amount = amount / buy;
                        Some(NewOrderRequest::new(pair, &format!("{amount:.6}"), side))
                    } else {
                        None
                    };
                    if let Some(req) = req {
                        let res = client.new_order(req.clone()).await.unwrap();
                        while client
                            .get_order(pair, res.data.order_id)
                            .await
                            .unwrap()
                            .data
                            .status
                            != "FULLY_FILLED"
                        {
                            tokio::time::sleep(tokio::time::Duration::from_secs_f32(0.2)).await;
                        }
                        let order = client.get_order(pair, res.data.order_id).await.unwrap();
                        if let Ok(url) = env::var("WEBHOOK_URL") {
                            let client = reqwest::Client::new();
                            let body = format!(
                                "注文約定: {}{}で{}{}を{}\n{}トリガー価格: {}\n予定価格{}\n24時間平均価格: {}",
                                order.data.average_price,
                                pair,
                                req.amount,
                                asset,
                                if side == "sell" { "売却" } else { "購入" },
                                if side == "sell" { "売却" } else { "購入" },
                                if side == "sell" {
                                    sell_trigger_price
                                } else {
                                    buy_trigger_price
                                },
                                if side == "sell" {
                                    sell
                                } else {
                                    buy
                                },
                                middle
                            );
                            let res = client.post(&url).body(body).send().await;
                            println!("{:?}", res);
                        }
                        break;
                    }
                }
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
    }
}
