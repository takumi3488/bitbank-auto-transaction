use hmac::{Hmac, Mac};
use sha2::Sha256;

#[derive(Debug)]
pub struct BitbankClient {
    client: reqwest::Client,
    endpoint: String,
    access_key: String,
    api_secret_key: String,
    access_time_window: u16,
}

#[derive(Debug, serde::Deserialize)]
pub struct GetAssetsResponse {
    pub data: Assets,
}

impl GetAssetsResponse {
    pub fn get_asset(&self, asset: &str) -> Option<&Asset> {
        self.data.assets.iter().find(|a| a.asset == asset)
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct Assets {
    pub assets: Vec<Asset>,
}

#[derive(Debug, serde::Deserialize)]
pub struct Asset {
    pub asset: String,
    pub free_amount: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct GetOrderResponse {
    pub data: Order,
}

#[derive(Debug, serde::Deserialize)]
pub struct Order {
    pub average_price: String,
    pub status: String,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct NewOrderRequest {
    pub pair: String,
    pub amount: String,
    pub side: String,
    #[serde(rename = "type")]
    pub type_: String, // market 固定
}

impl NewOrderRequest {
    pub fn new(pair: &str, amount: &str, side: &str) -> Self {
        Self {
            pair: pair.to_string(),
            amount: amount.to_string(),
            side: side.to_string(),
            type_: "market".to_string(),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct NewOrderResponse {
    pub data: NewOrder,
}

#[derive(Debug, serde::Deserialize)]
pub struct NewOrder {
    pub order_id: u64,
}

impl BitbankClient {
    pub fn new(access_key: &str, api_secret_key: &str, access_time_window: u16) -> Self {
        const ENDPOINT: &str = "https://api.bitbank.cc/v1";
        Self {
            client: reqwest::Client::new(),
            endpoint: ENDPOINT.to_string(),
            access_key: access_key.to_string(),
            api_secret_key: api_secret_key.to_string(),
            access_time_window,
        }
    }

    fn get_access_request_time(&self) -> i64 {
        let now = chrono::Utc::now();
        now.timestamp_millis()
    }

    fn get_access_signature(&self, s: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.api_secret_key.as_bytes()).unwrap();
        mac.update(s.as_bytes());
        let result = mac.finalize();
        let code_bytes = result.into_bytes();
        let mut code = String::new();
        for b in code_bytes.iter() {
            code.push_str(&format!("{:02x}", b));
        }
        code
    }

    async fn get<T>(&self, path: &str) -> Result<T, reqwest::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = format!("{}{}", self.endpoint, path);
        let access_request_time = self.get_access_request_time();
        let access_signature = self.get_access_signature(&format!(
            "{}{}/v1{}",
            access_request_time, self.access_time_window, path
        ));
        let response = self
            .client
            .get(&url)
            .header("ACCESS-KEY", self.access_key.as_str())
            .header("ACCESS-SIGNATURE", access_signature.to_ascii_lowercase())
            .header(
                "ACCESS-REQUEST-TIME",
                access_request_time.to_string().as_str(),
            )
            .header(
                "ACCESS-TIME-WINDOW",
                self.access_time_window.to_string().as_str(),
            )
            .send()
            .await?;
        response.json::<T>().await
    }

    async fn post<T, U>(&self, path: &str, body: &T) -> Result<U, reqwest::Error>
    where
        T: serde::Serialize,
        U: serde::de::DeserializeOwned,
    {
        let url = format!("{}{}", self.endpoint, path);
        let access_request_time = self.get_access_request_time();
        let body = serde_json::to_string(body).unwrap();
        let access_signature = self.get_access_signature(&format!(
            "{}{}{}",
            access_request_time, self.access_time_window, body
        ));
        let response = self
            .client
            .post(&url)
            .header("ACCESS-KEY", self.access_key.as_str())
            .header("ACCESS-SIGNATURE", access_signature.to_ascii_lowercase())
            .header(
                "ACCESS-REQUEST-TIME",
                access_request_time.to_string().as_str(),
            )
            .header(
                "ACCESS-TIME-WINDOW",
                self.access_time_window.to_string().as_str(),
            )
            .body(body)
            .send()
            .await?;
        response.json::<U>().await
    }

    pub async fn get_assets(&self) -> Result<GetAssetsResponse, reqwest::Error> {
        self.get("/user/assets").await
    }

    pub async fn new_order(
        &self,
        req: NewOrderRequest,
    ) -> Result<NewOrderResponse, reqwest::Error> {
        self.post("/user/spot/order", &req).await
    }

    pub async fn get_order(
        &self,
        pair: &str,
        order_id: u64,
    ) -> Result<GetOrderResponse, reqwest::Error> {
        self.get(&format!(
            "/user/spot/order?pair={}&order_id={}",
            pair, order_id
        ))
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_access_signature() {
        let client = BitbankClient::new("x", "hoge", 1000);
        let s = "17211217764901000/v1/user/assets";
        assert_eq!(
            client.get_access_signature(s),
            "9ec5745960d05573c8fb047cdd9191bd0c6ede26f07700bb40ecf1a3920abae8"
        );
    }
}
