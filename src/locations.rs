pub struct Location {
    pub id: &'static str,
    pub name: &'static str,
    pub name_en: &'static str,
    pub country: &'static str,
}

pub static LOCATIONS: [Location; 4] = [
    Location { id: "netherlands", name: "Нидерланды", name_en: "Netherlands", country: "NL" },
    Location { id: "germany", name: "Германия", name_en: "Germany", country: "DE" },
    Location { id: "poland", name: "Польша", name_en: "Poland", country: "PL" },
    Location { id: "usa", name: "США", name_en: "USA", country: "US" },
];
