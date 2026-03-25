// ============================================================
// FICHIER GÉNÉRÉ AUTOMATIQUEMENT — NE PAS MODIFIER À LA MAIN
//
// Généré par : tools/generate_radiofrance_stations.py
// Date       : 2026-03-25
// Source     : API GraphQL Radio France (openapi.radiofrance.fr)
// Token      : token de secours (AlexandrePradeilles/ECI, 2022)
// Stations   : 73
//
// Pour mettre à jour :
//   python3 tools/generate_radiofrance_stations.py
// ============================================================

pub const KNOWN_MAIN_STATIONS: &[(&str, &str)] = &[
    ("franceinter", "France Inter"),
    ("franceinfo", "franceinfo"),
    ("francemusique", "France Musique"),
    ("franceculture", "France Culture"),
    ("mouv", "Mouv\'"),
    ("fip", "FIP"),
];

/// Mapping slug → ID numérique livemeta/pull API
pub const STATION_IDS: &[(&str, u32)] = &[
    // Stations principales
    ("franceinter", 1),
    ("franceinfo", 2),
    ("francemusique", 4),
    ("franceculture", 5),
    ("mouv", 6),
    ("fip", 7),
    // Webradios
    ("franceinter_la_musique_inter", 1101),  // La musique d\'Inter
    ("francemusique_classique_easy", 401),  // Classique Easy
    ("francemusique_classique_plus", 402),  // Classique Plus
    ("francemusique_concert_rf", 403),  // Concerts Radio France
    ("francemusique_ocora_monde", 404),  // Ocora Musiques du Monde
    ("francemusique_la_jazz", 405),  // La Jazz
    ("francemusique_la_contemporaine", 406),  // La Contemporaine
    ("francemusique_la_bo", 407),  // Musique de Films
    ("francemusique_la_baroque", 408),  // La Baroque
    ("francemusique_opera", 409),  // Opéra
    ("francemusique_piano_zen", 410),  // Piano Zen
    ("fip_rock", 64),  // FIP Rock
    ("fip_jazz", 65),  // FIP Jazz
    ("fip_groove", 66),  // FIP Groove
    ("fip_world", 69),  // FIP Monde
    ("fip_nouveautes", 70),  // FIP Nouveautés
    ("fip_reggae", 71),  // FIP Reggae
    ("fip_electro", 74),  // FIP Electro
    ("fip_metal", 77),  // FIP Metal
    ("fip_pop", 78),  // FIP Pop
    ("fip_hiphop", 95),  // FIP Hip-Hop
    ("francebleu_chanson_francaise", 5601),  // 100% chanson française
    // Radios ICI (France Bleu locales)
    ("francebleu_rcfm", 11),  // ICI RCFM
    ("francebleu_alsace", 12),  // ICI Alsace
    ("francebleu_armorique", 13),  // ICI Armorique
    ("francebleu_auxerre", 14),  // ICI Auxerre
    ("francebleu_bearn", 15),  // ICI Béarn Bigorre
    ("francebleu_belfort_montbeliard", 16),  // ICI Belfort-Montbéliard
    ("francebleu_berry", 17),  // ICI Berry
    ("francebleu_besancon", 18),  // ICI Besançon
    ("francebleu_bourgogne", 19),  // ICI Bourgogne
    ("francebleu_breizh_izel", 20),  // ICI Breizh Izel
    ("francebleu_champagne_ardenne", 21),  // ICI Champagne-Ardenne
    ("francebleu_cotentin", 22),  // ICI Cotentin
    ("francebleu_creuse", 23),  // ICI Creuse
    ("francebleu_drome_ardeche", 24),  // ICI Drôme Ardèche
    ("francebleu_gard_lozere", 25),  // ICI Gard Lozère
    ("francebleu_gascogne", 26),  // ICI Gascogne
    ("francebleu_gironde", 27),  // ICI Gironde
    ("francebleu_herault", 28),  // ICI Hérault
    ("francebleu_isere", 29),  // ICI Isère
    ("francebleu_la_rochelle", 30),  // ICI La Rochelle
    ("francebleu_limousin", 31),  // ICI Limousin
    ("francebleu_loire_ocean", 32),  // ICI Loire Océan
    ("francebleu_lorraine", 33),  // ICI Lorraine (Meurthe-et-Moselle et Vosges)
    ("francebleu_mayenne", 34),  // ICI Mayenne
    ("francebleu_nord", 36),  // ICI Nord
    ("francebleu_normandie", 37),  // ICI Normandie (Calvados - Orne)
    ("francebleu_normandie_seine_maritime", 38),  // ICI Normandie (Seine-Maritime - Eure)
    ("francebleu_orleans", 39),  // ICI Orléans
    ("francebleu_pays_d_auvergne", 40),  // ICI Pays d\'Auvergne
    ("francebleu_pays_basque", 41),  // ICI Pays Basque
    ("francebleu_pays_de_savoie", 42),  // ICI Pays de Savoie
    ("francebleu_perigord", 43),  // ICI Périgord
    ("francebleu_picardie", 44),  // ICI Picardie
    ("francebleu_provence", 45),  // ICI Provence
    ("francebleu_roussillon", 46),  // ICI Roussillon
    ("francebleu_touraine", 47),  // ICI Touraine
    ("francebleu_vaucluse", 48),  // ICI Vaucluse
    ("francebleu_azur", 49),  // ICI Azur
    ("francebleu_lorraine_nord", 50),  // ICI Lorraine (Moselle et Pays Haut)
    ("francebleu_poitou", 54),  // ICI Poitou
    ("francebleu_paris", 68),  // ICI Paris Île-de-France
    ("francebleu_elsass", 90),  // ICI Elsass
    ("francebleu_maine", 91),  // ICI Maine
    ("francebleu_occitanie", 92),  // ICI Occitanie
    ("francebleu_saint_etienne_loire", 93),  // ICI Saint-Étienne Loire
];

/// URLs HiFi AAC Icecast (dérivées des URLs midfi de l'API GraphQL)
pub const STATION_STREAMS: &[(&str, &str)] = &[
    // Stations principales
    ("franceinter", "https://icecast.radiofrance.fr/franceinter-hifi.aac"),
    ("franceinfo", "https://icecast.radiofrance.fr/franceinfo-hifi.aac"),
    ("francemusique", "https://icecast.radiofrance.fr/francemusique-hifi.aac"),
    ("franceculture", "https://icecast.radiofrance.fr/franceculture-hifi.aac"),
    ("mouv", "https://icecast.radiofrance.fr/mouv-hifi.aac"),
    ("fip", "https://icecast.radiofrance.fr/fip-hifi.aac"),
    // Webradios
    ("franceinter_la_musique_inter", "https://icecast.radiofrance.fr/franceinterlamusiqueinter-hifi.aac"),  // La musique d\'Inter
    ("francemusique_classique_easy", "https://icecast.radiofrance.fr/francemusiqueeasyclassique-hifi.aac"),  // Classique Easy
    ("francemusique_classique_plus", "https://icecast.radiofrance.fr/francemusiqueclassiqueplus-hifi.aac"),  // Classique Plus
    ("francemusique_concert_rf", "https://icecast.radiofrance.fr/francemusiqueconcertsradiofrance-hifi.aac"),  // Concerts Radio France
    ("francemusique_ocora_monde", "https://icecast.radiofrance.fr/francemusiqueocoramonde-hifi.aac"),  // Ocora Musiques du Monde
    ("francemusique_la_jazz", "https://icecast.radiofrance.fr/francemusiquelajazz-hifi.aac"),  // La Jazz
    ("francemusique_la_contemporaine", "https://icecast.radiofrance.fr/francemusiquelacontemporaine-hifi.aac"),  // La Contemporaine
    ("francemusique_la_bo", "https://icecast.radiofrance.fr/francemusiquelabo-hifi.aac"),  // Musique de Films
    ("francemusique_la_baroque", "https://icecast.radiofrance.fr/francemusiquebaroque-hifi.aac"),  // La Baroque
    ("francemusique_opera", "https://icecast.radiofrance.fr/francemusiqueopera-hifi.aac"),  // Opéra
    ("francemusique_piano_zen", "https://icecast.radiofrance.fr/francemusiquepianozen-hifi.aac"),  // Piano Zen
    ("fip_rock", "https://icecast.radiofrance.fr/fiprock-hifi.aac"),  // FIP Rock
    ("fip_jazz", "https://icecast.radiofrance.fr/fipjazz-hifi.aac"),  // FIP Jazz
    ("fip_groove", "https://icecast.radiofrance.fr/fipgroove-hifi.aac"),  // FIP Groove
    ("fip_world", "https://icecast.radiofrance.fr/fipworld-hifi.aac"),  // FIP Monde
    ("fip_nouveautes", "https://icecast.radiofrance.fr/fipnouveautes-hifi.aac"),  // FIP Nouveautés
    ("fip_reggae", "https://icecast.radiofrance.fr/fipreggae-hifi.aac"),  // FIP Reggae
    ("fip_electro", "https://icecast.radiofrance.fr/fipelectro-hifi.aac"),  // FIP Electro
    ("fip_metal", "https://icecast.radiofrance.fr/fipmetal-hifi.aac"),  // FIP Metal
    ("fip_pop", "https://icecast.radiofrance.fr/fippop-hifi.aac"),  // FIP Pop
    ("fip_hiphop", "https://icecast.radiofrance.fr/fiphiphop-hifi.aac"),  // FIP Hip-Hop
    ("francebleu_chanson_francaise", "https://icecast.radiofrance.fr/fbchansonfrancaise-hifi.aac"),  // 100% chanson française
    // Radios ICI (France Bleu locales)
    ("francebleu_rcfm", "https://icecast.radiofrance.fr/fbfrequenzamora-hifi.aac"),  // ICI RCFM
    ("francebleu_alsace", "https://icecast.radiofrance.fr/fbalsace-hifi.aac"),  // ICI Alsace
    ("francebleu_armorique", "https://icecast.radiofrance.fr/fbarmorique-hifi.aac"),  // ICI Armorique
    ("francebleu_auxerre", "https://icecast.radiofrance.fr/fbauxerre-hifi.aac"),  // ICI Auxerre
    ("francebleu_bearn", "https://icecast.radiofrance.fr/fbbearn-hifi.aac"),  // ICI Béarn Bigorre
    ("francebleu_belfort_montbeliard", "https://icecast.radiofrance.fr/fbbelfort-hifi.aac"),  // ICI Belfort-Montbéliard
    ("francebleu_berry", "https://icecast.radiofrance.fr/fbberry-hifi.aac"),  // ICI Berry
    ("francebleu_besancon", "https://icecast.radiofrance.fr/fbbesancon-hifi.aac"),  // ICI Besançon
    ("francebleu_bourgogne", "https://icecast.radiofrance.fr/fbbourgogne-hifi.aac"),  // ICI Bourgogne
    ("francebleu_breizh_izel", "https://icecast.radiofrance.fr/fbbreizizel-hifi.aac"),  // ICI Breizh Izel
    ("francebleu_champagne_ardenne", "https://icecast.radiofrance.fr/fbchampagne-hifi.aac"),  // ICI Champagne-Ardenne
    ("francebleu_cotentin", "https://icecast.radiofrance.fr/fbcotentin-hifi.aac"),  // ICI Cotentin
    ("francebleu_creuse", "https://icecast.radiofrance.fr/fbcreuse-hifi.aac"),  // ICI Creuse
    ("francebleu_drome_ardeche", "https://icecast.radiofrance.fr/fbdromeardeche-hifi.aac"),  // ICI Drôme Ardèche
    ("francebleu_gard_lozere", "https://icecast.radiofrance.fr/fbgardlozere-hifi.aac"),  // ICI Gard Lozère
    ("francebleu_gascogne", "https://icecast.radiofrance.fr/fbgascogne-hifi.aac"),  // ICI Gascogne
    ("francebleu_gironde", "https://icecast.radiofrance.fr/fbgironde-hifi.aac"),  // ICI Gironde
    ("francebleu_herault", "https://icecast.radiofrance.fr/fbherault-hifi.aac"),  // ICI Hérault
    ("francebleu_isere", "https://icecast.radiofrance.fr/fbisere-hifi.aac"),  // ICI Isère
    ("francebleu_la_rochelle", "https://icecast.radiofrance.fr/fblarochelle-hifi.aac"),  // ICI La Rochelle
    ("francebleu_limousin", "https://icecast.radiofrance.fr/fblimousin-hifi.aac"),  // ICI Limousin
    ("francebleu_loire_ocean", "https://icecast.radiofrance.fr/fbloireocean-hifi.aac"),  // ICI Loire Océan
    ("francebleu_lorraine", "https://icecast.radiofrance.fr/fbsudlorraine-hifi.aac"),  // ICI Lorraine (Meurthe-et-Moselle et Vosges)
    ("francebleu_mayenne", "https://icecast.radiofrance.fr/fbmayenne-hifi.aac"),  // ICI Mayenne
    ("francebleu_nord", "https://icecast.radiofrance.fr/fbnord-hifi.aac"),  // ICI Nord
    ("francebleu_normandie", "https://icecast.radiofrance.fr/fbbassenormandie-hifi.aac"),  // ICI Normandie (Calvados - Orne)
    ("francebleu_normandie_seine_maritime", "https://icecast.radiofrance.fr/fbhautenormandie-hifi.aac"),  // ICI Normandie (Seine-Maritime - Eure)
    ("francebleu_orleans", "https://icecast.radiofrance.fr/fborleans-hifi.aac"),  // ICI Orléans
    ("francebleu_pays_d_auvergne", "https://icecast.radiofrance.fr/fbpaysdauvergne-hifi.aac"),  // ICI Pays d\'Auvergne
    ("francebleu_pays_basque", "https://icecast.radiofrance.fr/fbpaysbasque-hifi.aac"),  // ICI Pays Basque
    ("francebleu_pays_de_savoie", "https://icecast.radiofrance.fr/fbpaysdesavoie-hifi.aac"),  // ICI Pays de Savoie
    ("francebleu_perigord", "https://icecast.radiofrance.fr/fbperigord-hifi.aac"),  // ICI Périgord
    ("francebleu_picardie", "https://icecast.radiofrance.fr/fbpicardie-hifi.aac"),  // ICI Picardie
    ("francebleu_provence", "https://icecast.radiofrance.fr/fbprovence-hifi.aac"),  // ICI Provence
    ("francebleu_roussillon", "https://icecast.radiofrance.fr/fbroussillon-hifi.aac"),  // ICI Roussillon
    ("francebleu_touraine", "https://icecast.radiofrance.fr/fbtouraine-hifi.aac"),  // ICI Touraine
    ("francebleu_vaucluse", "https://icecast.radiofrance.fr/fbvaucluse-hifi.aac"),  // ICI Vaucluse
    ("francebleu_azur", "https://icecast.radiofrance.fr/fbazur-hifi.aac"),  // ICI Azur
    ("francebleu_lorraine_nord", "https://icecast.radiofrance.fr/fblorrainenord-hifi.aac"),  // ICI Lorraine (Moselle et Pays Haut)
    ("francebleu_poitou", "https://icecast.radiofrance.fr/fbpoitou-hifi.aac"),  // ICI Poitou
    ("francebleu_paris", "https://icecast.radiofrance.fr/fb1071-hifi.aac"),  // ICI Paris Île-de-France
    ("francebleu_elsass", "https://icecast.radiofrance.fr/fbelsass-hifi.aac"),  // ICI Elsass
    ("francebleu_maine", "https://icecast.radiofrance.fr/fbmaine-hifi.aac"),  // ICI Maine
    ("francebleu_occitanie", "https://icecast.radiofrance.fr/fbtoulouse-hifi.aac"),  // ICI Occitanie
    ("francebleu_saint_etienne_loire", "https://icecast.radiofrance.fr/fbstetienne-hifi.aac"),  // ICI Saint-Étienne Loire
];
