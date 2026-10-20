pub const CATEGORIES: [(&str, &str, &str); 9] = [
    ("applications-graphics-symbolic", "Arts", "purple"),
    ("applications-science-symbolic", "Science", "blue"),
    ("document-open-recent-symbolic", "News", "orange"),
    ("accessories-dictionary-symbolic", "Education", "green"),
    ("system-users-symbolic", "Society & Culture", "pink"),
    ("applications-games-symbolic", "Sports", "teal"),
    ("face-smile-symbolic", "Health", "red"),
    ("weather-few-clouds-symbolic", "Climate", "forest"),
    ("x-office-presentation-symbolic", "Business", "indigo"),
];

pub const RECOMMENDATION_GENRES: [(&str, u32); 20] = [
    ("Arts", 1301), ("Business", 1321), ("Comedy", 1303),
    ("Education", 1304), ("Fiction", 1483), ("Government", 1511),
    ("Health & Fitness", 1512), ("History", 1487), ("Kids & Family", 1305),
    ("Leisure", 1502), ("Music", 1310), ("News", 1489),
    ("Religion & Spirituality", 1314), ("Science", 1533),
    ("Society & Culture", 1324), ("Sports", 1545),
    ("Technology", 1318), ("TV & Film", 1309), ("True Crime", 1488),
    ("Earth Sciences", 1540),
];

type Subcategory = (&'static str, u32);

const ARTS: &[Subcategory] = &[
    ("Books", 1482), ("Design", 1402), ("Food", 1306), ("Performing Arts", 1405),
];
const SCIENCE: &[Subcategory] = &[
    ("Astronomy", 1538), ("Earth Sciences", 1540), ("Nature", 1537),
    ("Social Sciences", 1535),
];
const NEWS: &[Subcategory] = &[
    ("Daily News", 1526), ("Politics", 1527), ("Tech News", 1528),
    ("Business News", 1490),
];
const EDUCATION: &[Subcategory] = &[
    ("Courses", 1501), ("How To", 1499), ("Language Learning", 1498),
    ("Self-Improvement", 1500),
];
const SOCIETY: &[Subcategory] = &[
    ("Documentary", 1543), ("Philosophy", 1443), ("Places & Travel", 1320),
    ("Relationships", 1544),
];
const SPORTS: &[Subcategory] = &[
    ("Football", 1547), ("Soccer", 1546), ("Cricket", 1554), ("Basketball", 1548),
];
const HEALTH: &[Subcategory] = &[
    ("Fitness", 1514), ("Medicine", 1518), ("Mental Health", 1517),
    ("Nutrition", 1515),
];
const CLIMATE: &[Subcategory] = &[("Earth Sciences", 1540), ("Nature", 1537)];
const BUSINESS: &[Subcategory] = &[
    ("Entrepreneurship", 1493), ("Investing", 1412), ("Management", 1491),
    ("Marketing", 1492),
];

pub fn subcategories(parent: &str) -> &'static [Subcategory] {
    match parent {
        "Arts" => ARTS,
        "Science" => SCIENCE,
        "News" => NEWS,
        "Education" => EDUCATION,
        "Society & Culture" => SOCIETY,
        "Sports" => SPORTS,
        "Health" => HEALTH,
        "Climate" => CLIMATE,
        "Business" => BUSINESS,
        _ => &[],
    }
}

pub fn category_genre(parent: &str, selected: &str) -> Option<u32> {
    let main = match parent {
        "Arts" => 1301,
        "Science" => 1533,
        "News" => 1489,
        "Education" => 1304,
        "Society & Culture" => 1324,
        "Sports" => 1545,
        "Health" => 1512,
        "Climate" => 1540, // Apple's Earth Sciences subcategory.
        "Business" => 1321,
        _ => return None,
    };
    if selected == parent {
        Some(main)
    } else {
        subcategories(parent)
            .iter()
            .find(|(name, _)| *name == selected)
            .map(|(_, genre)| *genre)
    }
}
