use std::time::Duration;

use reqwest::Client;

const HTTP_TIMEOUT: Duration = Duration::from_secs(4);

pub async fn check_http(host: &str, port: usize) -> Result<u16, String> {
    let url = format!("http://{}:{}", host, port);

    let client = Client::builder()
        .timeout(HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .danger_accept_invalid_certs(true) // todo: self-signed certs are allowed ?
        .build()
        .map_err(|e| e.to_string())?;

    let res = client.get(url).send().await.map_err(|e| e.to_string())?;

    Ok(res.status().as_u16())
}
