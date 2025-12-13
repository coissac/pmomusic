//! Exemple de Spoofer Qobuz - Extraction dynamique des AppID et secrets
//!
//! Cet exemple reproduit le comportement du spoofer Python :
//! 1. Récupère la page de login Qobuz
//! 2. Extrait l'URL du bundle.js
//! 3. Télécharge le bundle
//! 4. Extrait l'AppID et les secrets via regex
//! 5. Décode les secrets en base64
//!
//! Usage:
//! ```bash
//! cargo run --example spoofer
//! ```

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use indexmap::IndexMap;
use regex::Regex;
use reqwest::Client;

struct Spoofer {
    bundle: String,
    seed_timezone_regex: Regex,
    info_extras_regex_template: String,
    app_id_regex: Regex,
}

impl Spoofer {
    /// Crée un nouveau Spoofer et télécharge le bundle.js
    async fn new() -> Result<Self> {
        // Expressions régulières (équivalent Python)
        let seed_timezone_regex = Regex::new(
            r#"[a-z]\.initialSeed\("(?P<seed>[\w=]+)",window\.utimezone\.(?P<timezone>[a-z]+)\)"#,
        )?;

        let info_extras_regex_template =
            r#"name:"\w+/(?P<timezone>{timezones})",info:"(?P<info>[\w=]+)",extras:"(?P<extras>[\w=]+)""#
                .to_string();

        let app_id_regex = Regex::new(
            r#"production:\{api:\{appId:"(?P<app_id>\d{9})",appSecret:"(?P<secret>\w{32})"\},braze:.\(.\(\{\},.\),\{\},\{apiKey:"([-0-9a-fA-F]{36})"\}\),extra:.\}"#,
        )?;

        // Créer un client HTTP
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; PMOMusic/1.0)")
            .build()?;

        println!("Récupération de la page de login...");
        let login_page = client
            .get("https://play.qobuz.com/login")
            .send()
            .await?
            .text()
            .await?;

        // Extraire l'URL du bundle
        let bundle_url_regex = Regex::new(
            r#"<script src="(/resources/\d+\.\d+\.\d+-[a-z]\d{3}/bundle\.js)"></script>"#,
        )?;
        let bundle_url = bundle_url_regex
            .captures(&login_page)
            .and_then(|cap| cap.get(1))
            .ok_or_else(|| anyhow::anyhow!("Impossible de trouver l'URL du bundle"))?
            .as_str();

        println!("Téléchargement du bundle depuis: {}", bundle_url);
        let bundle_full_url = format!("https://play.qobuz.com{}", bundle_url);
        let bundle = client.get(&bundle_full_url).send().await?.text().await?;

        println!("Bundle téléchargé ({} bytes)", bundle.len());

        Ok(Self {
            bundle,
            seed_timezone_regex,
            info_extras_regex_template,
            app_id_regex,
        })
    }

    /// Extrait l'App ID depuis le bundle
    fn get_app_id(&self) -> Result<String> {
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

    /// Extrait l'appSecret depuis le bundle (secret à 32 caractères)
    fn get_app_secret(&self) -> Result<String> {
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

    /// Extrait les secrets depuis le bundle
    fn get_secrets(&self) -> Result<IndexMap<String, String>> {
        // Étape 1: Extraire tous les seed/timezone pairs
        let mut secrets: IndexMap<String, Vec<String>> = IndexMap::new();

        for captures in self.seed_timezone_regex.captures_iter(&self.bundle) {
            let seed = captures
                .name("seed")
                .ok_or_else(|| anyhow::anyhow!("Groupe seed non trouvé"))?
                .as_str();
            let timezone = captures
                .name("timezone")
                .ok_or_else(|| anyhow::anyhow!("Groupe timezone non trouvé"))?
                .as_str();

            secrets
                .entry(timezone.to_string())
                .or_insert_with(Vec::new)
                .push(seed.to_string());
        }

        println!("Timezones trouvées: {:?}", secrets.keys());

        // Étape 2: Réordonner - on met la deuxième timezone en premier
        // (comme le fait le code Python avec move_to_end)
        if secrets.len() >= 2 {
            let keys: Vec<String> = secrets.keys().cloned().collect();
            let second_key = keys[1].clone();
            let second_value = secrets.get(&second_key).unwrap().clone();

            // Retirer et réinsérer pour le mettre en premier
            secrets.shift_remove(&second_key);
            let mut new_secrets = IndexMap::new();
            new_secrets.insert(second_key, second_value);
            for (k, v) in secrets {
                new_secrets.insert(k, v);
            }
            secrets = new_secrets;
        }

        // Étape 3: Construire la regex pour info/extras
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

        // Étape 4: Extraire info et extras pour chaque timezone
        for captures in info_extras_regex.captures_iter(&self.bundle) {
            let timezone_cap = captures
                .name("timezone")
                .ok_or_else(|| anyhow::anyhow!("Groupe timezone non trouvé"))?
                .as_str();
            let info = captures
                .name("info")
                .ok_or_else(|| anyhow::anyhow!("Groupe info non trouvé"))?
                .as_str();
            let extras = captures
                .name("extras")
                .ok_or_else(|| anyhow::anyhow!("Groupe extras non trouvé"))?
                .as_str();

            let timezone_lower = timezone_cap.to_lowercase();
            if let Some(vec) = secrets.get_mut(&timezone_lower) {
                vec.push(info.to_string());
                vec.push(extras.to_string());
            }
        }

        // Étape 5: Décoder les secrets en base64
        let mut decoded_secrets = IndexMap::new();
        for (timezone, parts) in secrets {
            let concatenated = parts.join("");

            // Retirer les 44 derniers caractères (comme Python [:-44])
            if concatenated.len() > 44 {
                let trimmed = &concatenated[..concatenated.len() - 44];

                // Décoder en base64
                match STANDARD.decode(trimmed) {
                    Ok(decoded_bytes) => match String::from_utf8(decoded_bytes) {
                        Ok(decoded_str) => {
                            decoded_secrets.insert(timezone, decoded_str);
                        }
                        Err(e) => {
                            eprintln!("Erreur UTF-8 pour timezone {}: {}", timezone, e);
                        }
                    },
                    Err(e) => {
                        eprintln!(
                            "Erreur de décodage base64 pour timezone {}: {}",
                            timezone, e
                        );
                    }
                }
            }
        }

        Ok(decoded_secrets)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialiser le logging
    tracing_subscriber::fmt::init();

    println!("=== Spoofer Qobuz ===\n");

    // Créer le spoofer
    let spoofer = Spoofer::new().await?;

    // Extraire l'App ID
    println!("\n--- App ID ---");
    match spoofer.get_app_id() {
        Ok(app_id) => println!("App ID: {}", app_id),
        Err(e) => eprintln!("Erreur lors de l'extraction de l'App ID: {}", e),
    }

    // Extraire l'appSecret (32 caractères)
    println!("\n--- AppSecret (32 chars) ---");
    match spoofer.get_app_secret() {
        Ok(secret) => println!("AppSecret: {}", secret),
        Err(e) => eprintln!("Erreur lors de l'extraction de l'AppSecret: {}", e),
    }

    // Extraire les secrets timezone
    println!("\n--- Secrets timezone (décodés base64) ---");
    match spoofer.get_secrets() {
        Ok(secrets) => {
            for (timezone, secret) in secrets {
                println!("{}: {}", timezone, secret);
            }
        }
        Err(e) => eprintln!("Erreur lors de l'extraction des secrets: {}", e),
    }

    Ok(())
}
