use serde::de::DeserializeOwned;

pub async fn fetch<T>(client: &reqwest::Client, url: &str) -> Result<T, reqwest::Error>
where
    T: DeserializeOwned,
{
    client.get(url)
        .header("accept", "application/activity+json")
        .send()
        .await?
        .json()
        .await
}
