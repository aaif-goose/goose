#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommandSource {
    Builtin,
    Recipe,
    Skill,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommandEntry {
    pub name: String,
    pub description: String,
    pub source: SlashCommandSource,
    pub input_hint: Option<String>,
}
