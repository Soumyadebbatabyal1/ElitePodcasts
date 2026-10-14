pub struct Podcast {
    pub title: &'static str,
    pub description: &'static str,
    pub artwork: &'static str,
}

// Fictional podcasts used by the UI shell until real discovery data is available.
pub const FEATURED: [Podcast; 3] = [
    Podcast { title: "The Quiet Hours", description: "Thoughtful conversations about the ideas that shape our everyday lives.", artwork: "assets/artwork/quiet-hours.svg" },
    Podcast { title: "Field Notes", description: "Stories from people building a more curious and connected world.", artwork: "assets/artwork/field-notes.svg" },
    Podcast { title: "Small Wonders", description: "A weekly collection of surprising stories from science and culture.", artwork: "assets/artwork/small-wonders.svg" },
];

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
