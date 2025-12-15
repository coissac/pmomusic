//! Test de signature MD5 avec timestamp fixe
//! Compare la signature générée par Rust avec celle de Python

use anyhow::Result;
use pmoqobuz::api::{signing, Spoofer};

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Test de signature MD5 ===\n");

    // Récupérer le secret depuis Spoofer (timezone secrets, comme Python)
    println!("1. Getting secret from Spoofer...");
    let spoofer = Spoofer::new().await?;
    let timezone_secrets = spoofer.get_secrets()?;
    let app_secret = timezone_secrets.values().next().unwrap(); // Premier secret (comme Python)
    println!("   ✓ Secret retrieved (length: {} bytes)", app_secret.len());
    println!("   Using timezone secret (like Python), not App Secret");

    // Paramètres du test
    let track_id = "19557883";
    let format_id = "27";
    let intent = "stream";

    // TIMESTAMP FIXE pour comparaison
    let timestamp = "1234567890.123456";

    println!("\n2. Computing signature with FIXED timestamp:");
    println!("   track_id: {}", track_id);
    println!("   format_id: {}", format_id);
    println!("   intent: {}", intent);
    println!("   timestamp: {}", timestamp);
    println!(
        "   secret: {}... (first 10 chars)",
        &app_secret[..10.min(app_secret.len())]
    );

    // Calculer la signature
    let signature = signing::sign_track_get_file_url(
        format_id,
        intent,
        track_id,
        timestamp,
        app_secret.as_bytes(),
    );

    println!("\n3. Result:");
    println!("   Signature: {}", signature);
    println!("\n✓ You can now compare this signature with Python using the same timestamp.");
    println!("\nPython test command:");
    println!("   python3 -c \"");
    println!("import hashlib");
    println!("secret = '{}'", app_secret);
    println!("ts = '{}'", timestamp);
    println!("s = 'trackgetFileUrlformat_id27intentstreamtrack_id19557883' + ts");
    println!("s = s.encode('ASCII') + secret.encode('utf-8')");
    println!("print('Signature:', hashlib.md5(s).hexdigest())");
    println!("   \"");

    Ok(())
}
