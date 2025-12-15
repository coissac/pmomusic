//! Outil CLI pour chiffrer/déchiffrer des mots de passe
//!
//! Usage:
//!   cargo run --example encrypt_password -- encrypt "mon_mot_de_passe"
//!   cargo run --example encrypt_password -- decrypt "encrypted:ABC123..."
//!   cargo run --example encrypt_password -- test

use anyhow::Result;
use pmoconfig::encryption::{decrypt_password, encrypt_password, get_password, is_encrypted};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "encrypt" => {
            if args.len() < 3 {
                eprintln!("Error: Missing password to encrypt");
                print_usage();
                return Ok(());
            }

            let password = &args[2];
            let encrypted = encrypt_password(password)?;

            println!("Original:  {}", password);
            println!("Encrypted: {}", encrypted);
            println!("\nAdd this to your config.yaml:");
            println!("password: \"{}\"", encrypted);
        }

        "decrypt" => {
            if args.len() < 3 {
                eprintln!("Error: Missing encrypted password");
                print_usage();
                return Ok(());
            }

            let encrypted = &args[2];

            if !is_encrypted(encrypted) {
                eprintln!("Error: Value does not start with 'encrypted:'");
                return Ok(());
            }

            match decrypt_password(encrypted) {
                Ok(password) => {
                    println!("Encrypted: {}", encrypted);
                    println!("Decrypted: {}", password);
                }
                Err(e) => {
                    eprintln!("Error: Failed to decrypt password");
                    eprintln!("This encrypted password was created on a different machine.");
                    eprintln!("Details: {}", e);
                }
            }
        }

        "test" => {
            println!("=== Password Encryption Test ===\n");

            // Test avec différents mots de passe
            let test_passwords = vec![
                "simple",
                "Complex_P@ssw0rd!",
                "très long mot de passe avec des caractères spéciaux: é à ç ê",
                "12345",
            ];

            for password in test_passwords {
                println!("Testing: {}", password);

                let encrypted = encrypt_password(password)?;
                println!("  Encrypted: {}", encrypted);

                let decrypted = decrypt_password(&encrypted)?;
                println!("  Decrypted: {}", decrypted);

                if password == decrypted {
                    println!("  ✓ Success!\n");
                } else {
                    println!("  ✗ FAILED! Passwords don't match!\n");
                    return Err(anyhow::anyhow!("Test failed"));
                }
            }

            // Test de la fonction get_password
            println!("=== Testing get_password() ===\n");

            let plaintext = get_password("plaintext_password")?;
            println!("Plaintext input: {}", plaintext);
            assert_eq!(plaintext, "plaintext_password");

            let encrypted_input = encrypt_password("secret123")?;
            let decrypted = get_password(&encrypted_input)?;
            println!("Encrypted input: {}", encrypted_input);
            println!("Auto-decrypted:  {}", decrypted);
            assert_eq!(decrypted, "secret123");

            println!("\n✓ All tests passed!");
        }

        _ => {
            eprintln!("Error: Unknown command '{}'", args[1]);
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    println!("Usage:");
    println!("  cargo run --example encrypt_password -- encrypt <password>");
    println!("  cargo run --example encrypt_password -- decrypt <encrypted>");
    println!("  cargo run --example encrypt_password -- test");
    println!("\nExamples:");
    println!("  cargo run --example encrypt_password -- encrypt \"MySecretPassword\"");
    println!("  cargo run --example encrypt_password -- decrypt \"encrypted:SGVsbG8gV29ybGQh...\"");
}
