use std::sync::LazyLock;

pub static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

pub mod dtos {
    use std::fmt::Display;

    use chrono::NaiveDate;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct Country {
        #[serde(rename = "countryCode")]
        pub country_code: String,
        pub name: String,
    }

    impl Display for Country {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{} ({})", self.name, self.country_code)
        }
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    pub struct PublicHoliday {
        pub date: NaiveDate,
        #[serde(rename = "localName")]
        pub local_name: String,
        pub name: String,
        #[serde(rename = "countryCode")]
        pub country_code: String,
        pub fixed: bool,
        pub global: bool,
        // Present if a holiday only applies to specific regions (e.g., "DE-BY")
        pub counties: Option<Vec<String>>,
        #[serde(rename = "launchYear")]
        pub launch_year: Option<i32>,
        #[serde(default, alias = "type")]
        pub types: Vec<String>,
    }
}

pub mod fetch {
    use reqwest::Client;

    use crate::http::dtos::{Country, PublicHoliday};

    /// see https://date.nager.at/swagger/index.html
    const BASE: &str = "https://date.nager.at/api/v3";

    pub async fn get_countries(
        client: &Client,
    ) -> Result<Vec<Country>, Box<dyn std::error::Error>> {
        let res = client
            .get(format!("{BASE}/AvailableCountries"))
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<Country>>()
            .await?;
        Ok(res)
    }

    pub async fn get_public_holidays(
        client: &Client,
        year: i32,
        country_code: &str,
    ) -> Result<Vec<PublicHoliday>, Box<dyn std::error::Error>> {
        let url = format!("{BASE}/PublicHolidays/{year}/{country_code}");
        let res = client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<PublicHoliday>>()
            .await?;
        Ok(res)
    }
}
