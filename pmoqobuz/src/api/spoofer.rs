use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use indexmap::IndexMap;
use regex::Regex;
use reqwest::Client;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Timeout par requête HTTP — le bundle fait ~7 MB et le CDN Qobuz peut être lent.
const BUNDLE_FETCH_TIMEOUT: Duration = Duration::from_secs(45);

/// Tentatives supplémentaires après un échec d'extraction.
const BUNDLE_EXTRACTION_RETRIES: usize = 2;

pub struct Spoofer {
    bundle: String,
    /// Version du bundle extrait, ex. `"8.1.0-b019"`.
    bundle_version: String,
    seed_timezone_regex: Regex,
    info_extras_regex_template: String,
    app_id_regex: Regex,
}

impl Spoofer {
    /// Version du bundle Qobuz actuellement chargé.
    pub fn bundle_version(&self) -> &str {
        &self.bundle_version
    }

    /// Récupère uniquement la version du bundle courant sans télécharger les 7 MB.
    ///
    /// Utile pour savoir si le bundle a changé avant de déclencher une extraction
    /// complète. Ne télécharge que la page de login (~5 KB).
    pub async fn fetch_current_bundle_version() -> Result<String> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; PMOMusic/1.0)")
            .timeout(BUNDLE_FETCH_TIMEOUT)
            .build()?;

        let login_page = client
            .get("https://play.qobuz.com/login")
            .send()
            .await?
            .text()
            .await?;

        let re = Regex::new(
            r#"<script src="/resources/(\d+\.\d+\.\d+-[a-z]\d{3})/bundle\.js"></script>"#,
        )?;
        re.captures(&login_page)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| anyhow::anyhow!("Version bundle introuvable dans la page de login"))
    }

    /// Télécharge et parse le bundle Qobuz. Retry jusqu'à `BUNDLE_EXTRACTION_RETRIES`
    /// fois en cas d'échec réseau ou d'extraction.
    pub async fn new() -> Result<Self> {
        let seed_timezone_regex = Regex::new(
            r#"[a-z]\.initialSeed\("(?P<seed>[\w=]+)",window\.utimezone\.(?P<timezone>[a-z]+)\)"#,
        )?;
        let info_extras_regex_template =
            r#"name:"\w+/(?P<timezone>{timezones})",info:"(?P<info>[\w=]+)",extras:"(?P<extras>[\w=]+)""#
                .to_string();
        let app_id_regex = Regex::new(
            r#"production:\{api:\{appId:"(?P<app_id>\d{9})",appSecret:"(?P<secret>\w{32})"\},braze:.\(.\(\{\},.\),\{\},\{apiKey:"([-0-9a-fA-F]{36})"\}\),extra:.\}"#,
        )?;

        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; PMOMusic/1.0)")
            .timeout(BUNDLE_FETCH_TIMEOUT)
            .build()?;

        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 1..=(BUNDLE_EXTRACTION_RETRIES + 1) {
            match Self::fetch_bundle(&client).await {
                Ok((bundle, bundle_version)) => {
                    info!("[Spoofer] Bundle {} téléchargé ({} bytes)", bundle_version, bundle.len());
                    return Ok(Self {
                        bundle,
                        bundle_version,
                        seed_timezone_regex,
                        info_extras_regex_template,
                        app_id_regex,
                    });
                }
                Err(e) => {
                    warn!("[Spoofer] Tentative {}/{} échouée : {}", attempt, BUNDLE_EXTRACTION_RETRIES + 1, e);
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Échec téléchargement bundle")))
    }

    async fn fetch_bundle(client: &Client) -> Result<(String, String)> {
        let login_page = client
            .get("https://play.qobuz.com/login")
            .send()
            .await?
            .text()
            .await?;

        let bundle_url_regex = Regex::new(
            r#"<script src="(/resources/(\d+\.\d+\.\d+-[a-z]\d{3})/bundle\.js)"></script>"#,
        )?;
        let caps = bundle_url_regex
            .captures(&login_page)
            .ok_or_else(|| anyhow::anyhow!("URL bundle introuvable dans la page de login"))?;
        let bundle_path = caps.get(1).unwrap().as_str();
        let bundle_version = caps.get(2).unwrap().as_str().to_string();

        let bundle_url = format!("https://play.qobuz.com{}", bundle_path);
        debug!("[Spoofer] Téléchargement bundle depuis {}", bundle_url);

        let bundle = client.get(&bundle_url).send().await?.text().await?;
        Ok((bundle, bundle_version))
    }

    /// Extrait l'App ID depuis le bundle.
    pub fn get_app_id(&self) -> Result<String> {
        let captures = self
            .app_id_regex
            .captures(&self.bundle)
            .ok_or_else(|| anyhow::anyhow!("AppID non trouvé dans le bundle"))?;

        Ok(captures
            .name("app_id")
            .ok_or_else(|| anyhow::anyhow!("Groupe app_id non trouvé"))?
            .as_str()
            .to_string())
    }

    /// Extrait l'appSecret depuis le bundle (secret MD5 à 32 caractères).
    pub fn get_app_secret(&self) -> Result<String> {
        let captures = self
            .app_id_regex
            .captures(&self.bundle)
            .ok_or_else(|| anyhow::anyhow!("AppSecret non trouvé dans le bundle"))?;

        Ok(captures
            .name("secret")
            .ok_or_else(|| anyhow::anyhow!("Groupe secret non trouvé"))?
            .as_str()
            .to_string())
    }

    /// Extrait les secrets timezone depuis le bundle.
    pub fn get_secrets(&self) -> Result<IndexMap<String, String>> {
        let mut secrets: IndexMap<String, Vec<String>> = IndexMap::new();

        for captures in self.seed_timezone_regex.captures_iter(&self.bundle) {
            let seed = captures.name("seed").unwrap().as_str();
            let timezone = captures.name("timezone").unwrap().as_str();
            secrets
                .entry(timezone.to_string())
                .or_default()
                .push(seed.to_string());
        }

        debug!("[Spoofer] Timezones trouvées : {:?}", secrets.keys().collect::<Vec<_>>());

        if secrets.len() >= 2 {
            let keys: Vec<String> = secrets.keys().cloned().collect();
            let second_key = keys[1].clone();
            let second_value = secrets.get(&second_key).unwrap().clone();
            secrets.shift_remove(&second_key);
            let mut new_secrets = IndexMap::new();
            new_secrets.insert(second_key, second_value);
            for (k, v) in secrets {
                new_secrets.insert(k, v);
            }
            secrets = new_secrets;
        }

        let timezones_capitalized: Vec<String> = secrets
            .keys()
            .map(|tz| {
                let mut chars = tz.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect();

        let info_extras_regex_str = self
            .info_extras_regex_template
            .replace("{timezones}", &timezones_capitalized.join("|"));
        let info_extras_regex = Regex::new(&info_extras_regex_str)?;

        for captures in info_extras_regex.captures_iter(&self.bundle) {
            let timezone_cap = captures.name("timezone").unwrap().as_str();
            let info = captures.name("info").unwrap().as_str();
            let extras = captures.name("extras").unwrap().as_str();
            let timezone_lower = timezone_cap.to_lowercase();
            if let Some(vec) = secrets.get_mut(&timezone_lower) {
                vec.push(info.to_string());
                vec.push(extras.to_string());
            }
        }

        let mut decoded_secrets = IndexMap::new();
        for (timezone, parts) in secrets {
            let concatenated = parts.join("");
            if concatenated.len() > 44 {
                let trimmed = &concatenated[..concatenated.len() - 44];
                match STANDARD.decode(trimmed) {
                    Ok(decoded_bytes) => match String::from_utf8(decoded_bytes) {
                        Ok(decoded_str) => {
                            decoded_secrets.insert(timezone, decoded_str);
                        }
                        Err(e) => warn!("[Spoofer] UTF-8 invalide pour timezone {}: {}", timezone, e),
                    },
                    Err(e) => warn!("[Spoofer] Base64 invalide pour timezone {}: {}", timezone, e),
                }
            }
        }

        Ok(decoded_secrets)
    }
}
