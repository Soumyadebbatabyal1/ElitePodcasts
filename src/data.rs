pub struct Podcast {
    pub title: &'static str,
    pub publisher: &'static str,
    pub description: &'static str,
    pub artwork: &'static str,
}

pub const FEATURED: [Podcast; 3] = [
    Podcast { title: "The Quiet Hours", publisher: "Northstar Audio", description: "Thoughtful conversations about the ideas that shape our everyday lives.", artwork: "assets/artwork/quiet-hours.svg" },
    Podcast { title: "Field Notes", publisher: "Common Ground", description: "Stories from people building a more curious and connected world.", artwork: "assets/artwork/field-notes.svg" },
    Podcast { title: "Small Wonders", publisher: "Daybreak Studio", description: "A weekly collection of surprising stories from science and culture.", artwork: "assets/artwork/small-wonders.svg" },
];

pub const TRENDING: [Podcast; 4] = [
    Podcast { title: "The Daily Practice", publisher: "Studio Meridian", description: "A little inspiration for the work in front of you.", artwork: "assets/artwork/daily-practice.svg" },
    Podcast { title: "Off the Map", publisher: "Open Road Media", description: "Travel stories that take the scenic route.", artwork: "assets/artwork/off-the-map.svg" },
    Podcast { title: "Good Questions", publisher: "Public Radio Lab", description: "Big questions, asked with care and curiosity.", artwork: "assets/artwork/good-questions.svg" },
    Podcast { title: "After the Signal", publisher: "Signal House", description: "Culture, technology, and what comes next.", artwork: "assets/artwork/after-signal.svg" },
];

pub const CATEGORIES: [(&str, &str, &str); 8] = [
    ("applications-graphics-symbolic", "Arts", "purple"),
    ("applications-science-symbolic", "Science", "blue"),
    ("document-open-recent-symbolic", "News", "orange"),
    ("accessories-dictionary-symbolic", "Education", "green"),
    ("system-users-symbolic", "Society & Culture", "pink"),
    ("applications-games-symbolic", "Sports", "teal"),
    ("face-smile-symbolic", "Comedy", "red"),
    ("starred-symbolic", "Health", "yellow"),
];
