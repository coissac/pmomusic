#!/usr/bin/env python3
"""
Génère les constantes Rust pour pmoradiofrance depuis l'API GraphQL Radio France.

Usage:
    python3 generate_radiofrance_stations.py --token <votre-token>
    python3 generate_radiofrance_stations.py  # utilise RADIOFRANCE_TOKEN env var

Token de secours (token public trouvé dans AlexandrePradeilles/ECI, actif depuis 2022) :
    36bee04f-68a9-4bf8-8f2c-0662b454192c

Le script interroge l'API GraphQL officielle et génère le code Rust pour :
- KNOWN_MAIN_STATIONS : stations principales (slug, nom officiel)
- STATION_IDS         : slug → ID numérique livemeta/pull
- STATION_STREAMS     : slug → URL HiFi AAC Icecast
"""

import argparse
import os
import sys
import json
import urllib.request
import urllib.parse
import re

GRAPHQL_URL = "https://openapi.radiofrance.fr/v1/graphql"

# Token de secours — trouvé dans le dépôt public AlexandrePradeilles/ECI (oct. 2022),
# toujours actif. À remplacer par ton propre token si révoqué.
FALLBACK_TOKEN = "36bee04f-68a9-4bf8-8f2c-0662b454192c"

QUERY = """
{
  brands {
    id
    title
    playerUrl
    liveStream
    webRadios {
      id
      title
      playerUrl
      liveStream
    }
    localRadios {
      id
      title
      playerUrl
      liveStream
    }
  }
}
"""

# Slugs manuels pour les cas où la dérivation automatique ne correspond pas
# à la convention déjà établie dans le code Rust.
# Format : GraphQL_ID → slug_rust
SLUG_OVERRIDES = {
    # LocalRadios dont l'ID GraphQL manque le préfixe francebleu_
    "ELSASS": "francebleu_elsass",
    # Stations dont le slug GraphQL diffère du slug Rust historique
    "FRANCEBLEU_SUR_LORRAINE": "francebleu_lorraine",
    "FRANCEBLEU_NORMANDIE_CAEN": "francebleu_normandie",
    "FRANCEBLEU_NORMANDIE_ROUEN": "francebleu_normandie_seine_maritime",
    "FRANCEBLEU_TOULOUSE": "francebleu_occitanie",
}

# Slugs qui ne correspondent pas à la conversion lowercase de l'ID GraphQL
SLUG_OVERRIDES.update({
    "FIP_HIP_HOP": "fip_hiphop",
})

# Mouv' webradios : pas de liveStream donc inutilisables, on les exclut
EXCLUDED_WEBRADIO_IDS = {
    "MOUV_100MIX",
    "MOUV_CLASSICS",
    "MOUV_DANCEHALL",
    "MOUV_RNB",
    "MOUV_RAPUS",
    "MOUV_RAPFR",
}

# Brands à exclure en tant que station principale (pas de stream au niveau brand)
# mais dont on garde les webRadios et localRadios
EXCLUDED_BRAND_STREAM_IDS = {"FRANCEBLEU"}


def graphql_id_to_slug(graphql_id: str, is_local_radio: bool = False) -> str:
    """Convertit un ID GraphQL en slug Rust."""
    if graphql_id in SLUG_OVERRIDES:
        return SLUG_OVERRIDES[graphql_id]

    slug = graphql_id.lower()

    # LocalRadios sous FRANCEBLEU qui n'ont pas le préfixe
    if is_local_radio and not slug.startswith("francebleu_"):
        slug = "francebleu_" + slug

    return slug


def midfi_to_hifi(stream_url: str) -> str | None:
    """Convertit une URL midfi MP3 en URL HiFi AAC."""
    if not stream_url:
        return None
    # https://icecast.radiofrance.fr/fiprock-midfi.mp3?id=openapi
    # → https://icecast.radiofrance.fr/fiprock-hifi.aac
    url = re.sub(r"-midfi\.mp3\?.*$", "-hifi.aac", stream_url)
    if url == stream_url:
        return None  # pattern non reconnu
    return url


def extract_livemeta_id(player_url: str | None) -> int | None:
    """Extrait l'ID numérique depuis l'URL du player embed."""
    if not player_url:
        return None
    m = re.search(r"id_station=(\d+)", player_url)
    return int(m.group(1)) if m else None


def query_api(token: str) -> dict:
    url = f"{GRAPHQL_URL}?x-token={token}"
    payload = json.dumps({"query": QUERY}).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=payload,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=15) as resp:
        return json.loads(resp.read())


def collect_stations(data: dict) -> list[dict]:
    """
    Retourne une liste plate de stations :
    { slug, name, livemeta_id, hifi_url, kind }
    kind ∈ { "brand", "webradio", "local" }
    """
    stations = []

    for brand in data["data"]["brands"]:
        brand_id = brand["id"]

        # Station principale — seulement si elle a un stream au niveau brand
        if brand_id not in EXCLUDED_BRAND_STREAM_IDS:
            livemeta_id = extract_livemeta_id(brand.get("playerUrl"))
            hifi_url = midfi_to_hifi(brand.get("liveStream") or "")
            if not hifi_url:
                slug = graphql_id_to_slug(brand_id)
                hifi_url = f"https://icecast.radiofrance.fr/{slug}-hifi.aac"
            stations.append({
                "slug": graphql_id_to_slug(brand_id),
                "name": brand["title"],
                "livemeta_id": livemeta_id,
                "hifi_url": hifi_url,
                "kind": "brand",
            })

        # Webradios
        for wr in brand.get("webRadios") or []:
            if wr["id"] in EXCLUDED_WEBRADIO_IDS:
                continue
            live = wr.get("liveStream")
            hifi = midfi_to_hifi(live or "")
            if not hifi:
                continue  # pas de stream utilisable
            stations.append({
                "slug": graphql_id_to_slug(wr["id"]),
                "name": wr["title"],
                "livemeta_id": extract_livemeta_id(wr.get("playerUrl")),
                "hifi_url": hifi,
                "kind": "webradio",
            })

        # Radios locales (ICI / France Bleu)
        for lr in brand.get("localRadios") or []:
            live = lr.get("liveStream")
            hifi = midfi_to_hifi(live or "")
            if not hifi:
                continue
            stations.append({
                "slug": graphql_id_to_slug(lr["id"], is_local_radio=True),
                "name": lr["title"],
                "livemeta_id": extract_livemeta_id(lr.get("playerUrl")),
                "hifi_url": hifi,
                "kind": "local",
            })

    return stations


def rust_escape(s: str) -> str:
    return s.replace('"', '\\"').replace("'", "\\'")


def generate_rust(stations: list[dict], token_origin: str = "inconnu") -> str:
    brands = [s for s in stations if s["kind"] == "brand"]
    webradios = [s for s in stations if s["kind"] == "webradio"]
    locals_ = [s for s in stations if s["kind"] == "local"]

    from datetime import date

    lines = []
    lines.append("// ============================================================")
    lines.append("// FICHIER GÉNÉRÉ AUTOMATIQUEMENT — NE PAS MODIFIER À LA MAIN")
    lines.append("//")
    lines.append(f"// Généré par : tools/generate_radiofrance_stations.py")
    lines.append(f"// Date       : {date.today().isoformat()}")
    lines.append(f"// Source     : API GraphQL Radio France (openapi.radiofrance.fr)")
    lines.append(f"// Token      : {token_origin}")
    lines.append(f"// Stations   : {len(stations)}")
    lines.append("//")
    lines.append("// Pour mettre à jour :")
    lines.append("//   python3 tools/generate_radiofrance_stations.py")
    lines.append("// ============================================================")
    lines.append("")

    # KNOWN_MAIN_STATIONS
    lines.append("pub const KNOWN_MAIN_STATIONS: &[(&str, &str)] = &[")
    for s in brands:
        lines.append(f'    ("{s["slug"]}", "{rust_escape(s["name"])}"),')
    lines.append("];")
    lines.append("")

    # STATION_IDS
    lines.append("/// Mapping slug → ID numérique livemeta/pull API")
    lines.append("pub const STATION_IDS: &[(&str, u32)] = &[")

    lines.append("    // Stations principales")
    for s in brands:
        if s["livemeta_id"] is not None:
            lines.append(f'    ("{s["slug"]}", {s["livemeta_id"]}),')

    lines.append("    // Webradios")
    for s in webradios:
        if s["livemeta_id"] is not None:
            lines.append(f'    ("{s["slug"]}", {s["livemeta_id"]}),  // {rust_escape(s["name"])}')

    lines.append("    // Radios ICI (France Bleu locales)")
    for s in sorted(locals_, key=lambda x: x["livemeta_id"] or 0):
        if s["livemeta_id"] is not None:
            lines.append(f'    ("{s["slug"]}", {s["livemeta_id"]}),  // {rust_escape(s["name"])}')

    lines.append("];")
    lines.append("")

    # STATION_STREAMS
    lines.append("/// URLs HiFi AAC Icecast (dérivées des URLs midfi de l'API GraphQL)")
    lines.append("pub const STATION_STREAMS: &[(&str, &str)] = &[")

    lines.append("    // Stations principales")
    for s in brands:
        lines.append(f'    ("{s["slug"]}", "{s["hifi_url"]}"),')

    lines.append("    // Webradios")
    for s in webradios:
        lines.append(f'    ("{s["slug"]}", "{s["hifi_url"]}"),  // {rust_escape(s["name"])}')

    lines.append("    // Radios ICI (France Bleu locales)")
    for s in sorted(locals_, key=lambda x: x["livemeta_id"] or 0):
        lines.append(f'    ("{s["slug"]}", "{s["hifi_url"]}"),  // {rust_escape(s["name"])}')

    lines.append("];")

    return "\n".join(lines)


SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
DEFAULT_OUTPUT = os.path.join(SCRIPT_DIR, "..", "pmoradiofrance", "src", "stations_data.rs")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--token", help="Token API Radio France (ou variable RADIOFRANCE_TOKEN)")
    parser.add_argument(
        "--output", "-o",
        default=DEFAULT_OUTPUT,
        help=f"Fichier Rust de sortie (défaut : {DEFAULT_OUTPUT})",
    )
    args = parser.parse_args()

    token = args.token or os.environ.get("RADIOFRANCE_TOKEN") or FALLBACK_TOKEN
    if token == FALLBACK_TOKEN:
        token_origin = "token de secours (AlexandrePradeilles/ECI, 2022)"
        print("Info : utilisation du token de secours (AlexandrePradeilles/ECI).", file=sys.stderr)
    elif os.environ.get("RADIOFRANCE_TOKEN"):
        token_origin = "variable d'environnement RADIOFRANCE_TOKEN"
    else:
        token_origin = "argument --token"

    print("Interrogation de l'API GraphQL Radio France...", file=sys.stderr)
    try:
        data = query_api(token)
    except Exception as e:
        print(f"Erreur API : {e}", file=sys.stderr)
        sys.exit(1)

    if "errors" in data:
        print(f"Erreurs GraphQL : {data['errors']}", file=sys.stderr)
        sys.exit(1)

    stations = collect_stations(data)
    print(f"{len(stations)} stations collectées.", file=sys.stderr)

    rust_code = generate_rust(stations, token_origin=token_origin)

    output_path = os.path.realpath(args.output)
    with open(output_path, "w", encoding="utf-8") as f:
        f.write(rust_code + "\n")
    print(f"Fichier écrit : {output_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
