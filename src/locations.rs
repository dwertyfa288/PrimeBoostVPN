pub struct LocationMeta {
    pub name: String,
    pub name_en: String,
    pub country: String,
}

const KNOWN: [(&str, &str, &str, &str); 20] = [
    ("netherlands", "Нидерланды", "Netherlands", "NL"),
    ("germany", "Германия", "Germany", "DE"),
    ("poland", "Польша", "Poland", "PL"),
    ("usa", "США", "USA", "US"),
    ("finland", "Финляндия", "Finland", "FI"),
    ("sweden", "Швеция", "Sweden", "SE"),
    ("france", "Франция", "France", "FR"),
    ("uk", "Великобритания", "United Kingdom", "GB"),
    ("kazakhstan", "Казахстан", "Kazakhstan", "KZ"),
    ("turkey", "Турция", "Turkey", "TR"),
    ("latvia", "Латвия", "Latvia", "LV"),
    ("estonia", "Эстония", "Estonia", "EE"),
    ("lithuania", "Литва", "Lithuania", "LT"),
    ("austria", "Австрия", "Austria", "AT"),
    ("switzerland", "Швейцария", "Switzerland", "CH"),
    ("japan", "Япония", "Japan", "JP"),
    ("singapore", "Сингапур", "Singapore", "SG"),
    ("russia", "Россия", "Russia", "RU"),
    ("bulgaria", "Болгария", "Bulgaria", "BG"),
    ("israel", "Израиль", "Israel", "IL"),
];

pub fn describe(id: &str) -> LocationMeta {
    match KNOWN.iter().find(|(known, ..)| *known == id) {
        Some((_, name, name_en, country)) => LocationMeta {
            name: (*name).to_owned(),
            name_en: (*name_en).to_owned(),
            country: (*country).to_owned(),
        },
        None => LocationMeta {
            name: id.to_owned(),
            name_en: id.to_owned(),
            country: id.chars().take(2).collect::<String>().to_uppercase(),
        },
    }
}
