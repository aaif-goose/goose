//! Build-time branding seam for downstream distributions.
//!
//! Every user-visible occurrence of the product noun inside this crate routes
//! through [`Brand::get()`]. Downstream distros override any field by setting
//! the corresponding `GOOSE_BRAND_*` env var at `cargo build` time; unset vars
//! fall back to today's goose-branded defaults.
//!
//! For clap `about` / `long_about` strings embedded in derive attributes, the
//! derive keeps the literal goose-branded text and [`apply_branding`] walks the
//! `clap::Command` tree at startup, rewriting product-noun tokens when the brand
//! differs from the default. When [`Brand::is_default`] is true the pass is a
//! no-op — default builds produce byte-identical `--help`, manpages, and shell
//! templates.
//!
//! See `CUSTOM_DISTROS.md` for the env var list and usage.

const DEFAULT_PRODUCT_NAME: &str = "goose";
const DEFAULT_BINARY_NAME: &str = "goose";
const DEFAULT_SHELL_ALIAS_PRIMARY: &str = "goose";
const DEFAULT_SHELL_ALIAS_SHORT: &str = "g";
const DEFAULT_SHELL_FN_PREFIX: &str = "goose";
const DEFAULT_DEEPLINK_SCHEME: &str = "goose";
const DEFAULT_GITHUB_OWNER: &str = "aaif-goose";
const DEFAULT_GITHUB_REPO: &str = "goose";
const DEFAULT_AGENT_IDENTITY: &str = "You are goose, an AI assistant.";

pub struct Brand {
    /// Display noun used in user-facing messages (e.g. "goose Version:").
    pub product_name: &'static str,
    /// Compiled binary name, clap root command name, invocation examples.
    pub binary_name: &'static str,
    /// Primary shell alias emitted by `term init` (e.g. `@goose`).
    pub shell_alias_primary: &'static str,
    /// Short shell alias emitted by `term init` (e.g. `@g`).
    pub shell_alias_short: &'static str,
    /// Function/variable prefix used in shell templates
    /// (e.g. `goose_preexec`, `goose_preexec_installed`).
    pub shell_fn_prefix: &'static str,
    /// URL scheme for recipe deeplinks (e.g. `goose://recipe?...`).
    pub deeplink_scheme: &'static str,
    /// GitHub owner for the update + attestation URLs.
    pub github_owner: &'static str,
    /// GitHub repo for the update + attestation URLs.
    pub github_repo: &'static str,
    /// System prompt used by the provider-configuration smoke test.
    pub agent_identity_sentence: &'static str,
}

pub const BRAND: Brand = Brand {
    product_name: match option_env!("GOOSE_BRAND_PRODUCT_NAME") {
        Some(v) => v,
        None => DEFAULT_PRODUCT_NAME,
    },
    binary_name: match option_env!("GOOSE_BRAND_BINARY_NAME") {
        Some(v) => v,
        None => DEFAULT_BINARY_NAME,
    },
    shell_alias_primary: match option_env!("GOOSE_BRAND_SHELL_ALIAS_PRIMARY") {
        Some(v) => v,
        None => DEFAULT_SHELL_ALIAS_PRIMARY,
    },
    shell_alias_short: match option_env!("GOOSE_BRAND_SHELL_ALIAS_SHORT") {
        Some(v) => v,
        None => DEFAULT_SHELL_ALIAS_SHORT,
    },
    shell_fn_prefix: match option_env!("GOOSE_BRAND_SHELL_FN_PREFIX") {
        Some(v) => v,
        None => DEFAULT_SHELL_FN_PREFIX,
    },
    deeplink_scheme: match option_env!("GOOSE_BRAND_DEEPLINK_SCHEME") {
        Some(v) => v,
        None => DEFAULT_DEEPLINK_SCHEME,
    },
    github_owner: match option_env!("GOOSE_BRAND_GITHUB_OWNER") {
        Some(v) => v,
        None => DEFAULT_GITHUB_OWNER,
    },
    github_repo: match option_env!("GOOSE_BRAND_GITHUB_REPO") {
        Some(v) => v,
        None => DEFAULT_GITHUB_REPO,
    },
    agent_identity_sentence: match option_env!("GOOSE_BRAND_AGENT_IDENTITY") {
        Some(v) => v,
        None => DEFAULT_AGENT_IDENTITY,
    },
};

impl Brand {
    pub fn get() -> &'static Brand {
        &BRAND
    }

    pub fn is_default(&self) -> bool {
        self.product_name == DEFAULT_PRODUCT_NAME
            && self.binary_name == DEFAULT_BINARY_NAME
            && self.shell_alias_primary == DEFAULT_SHELL_ALIAS_PRIMARY
            && self.shell_alias_short == DEFAULT_SHELL_ALIAS_SHORT
            && self.shell_fn_prefix == DEFAULT_SHELL_FN_PREFIX
            && self.deeplink_scheme == DEFAULT_DEEPLINK_SCHEME
            && self.github_owner == DEFAULT_GITHUB_OWNER
            && self.github_repo == DEFAULT_GITHUB_REPO
            && self.agent_identity_sentence == DEFAULT_AGENT_IDENTITY
    }

    pub fn product_name_cap(&self) -> String {
        capitalize(self.product_name)
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Rewrite the default product noun to the branded equivalents.
///
/// Replaces `Goose` with the capitalized product name and `goose` with the
/// binary name. Downstream distros that keep `product_name` in sync with the
/// title-cased `binary_name` (the common case) get sensible output for both
/// display phrases ("Configure goose settings") and invocation examples
/// ("goose term init zsh").
fn rebrand_str(s: &str, b: &Brand) -> String {
    let product_cap = b.product_name_cap();
    s.replace("Goose", &product_cap)
        .replace(DEFAULT_BINARY_NAME, b.binary_name)
}

/// Walk the clap command tree and rewrite branding-sensitive strings.
///
/// On the default build this returns the input unchanged, preserving
/// byte-identical `--help` output.
pub fn apply_branding(cmd: clap::Command) -> clap::Command {
    let b = Brand::get();
    if b.is_default() {
        return cmd;
    }
    rewrite_command(cmd, b)
}

fn rewrite_command(mut cmd: clap::Command, b: &Brand) -> clap::Command {
    if cmd.get_name() == DEFAULT_BINARY_NAME {
        cmd = cmd.name(b.binary_name).bin_name(b.binary_name);
    }

    if let Some(about) = cmd.get_about().map(|s| s.to_string()) {
        cmd = cmd.about(rebrand_str(&about, b));
    }
    if let Some(long_about) = cmd.get_long_about().map(|s| s.to_string()) {
        cmd = cmd.long_about(rebrand_str(&long_about, b));
    }

    if cmd.get_name() == "completion" {
        cmd = cmd.mut_arg("bin_name", |a| a.default_value(b.binary_name));
    }

    // Rewrite branded tokens inside every argument's help / long_help.
    let arg_ids: Vec<clap::Id> = cmd.get_arguments().map(|a| a.get_id().clone()).collect();
    for id in arg_ids {
        cmd = cmd.mut_arg(id, |a| rewrite_arg(a, b));
    }

    let subcmd_names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_string())
        .collect();
    for name in subcmd_names {
        cmd = cmd.mut_subcommand(name, |sc| rewrite_command(sc, b));
    }

    cmd
}

fn rewrite_arg(mut arg: clap::Arg, b: &Brand) -> clap::Arg {
    if let Some(help) = arg.get_help().map(|s| s.to_string()) {
        arg = arg.help(rebrand_str(&help, b));
    }
    if let Some(long_help) = arg.get_long_help().map(|s| s.to_string()) {
        arg = arg.long_help(rebrand_str(&long_help, b));
    }
    arg
}

/// Build the branded top-level clap command.
///
/// Use this in place of `Cli::command()` at every entry point (the main binary
/// and the manpage generator) so branding is applied consistently.
pub fn branded_command() -> clap::Command {
    use clap::CommandFactory;
    apply_branding(crate::Cli::command())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_brand_matches_hardcoded_defaults() {
        let b = Brand::get();
        assert!(b.is_default(), "default build must have all defaults");
        assert_eq!(b.product_name, "goose");
        assert_eq!(b.binary_name, "goose");
        assert_eq!(b.shell_alias_primary, "goose");
        assert_eq!(b.shell_alias_short, "g");
        assert_eq!(b.shell_fn_prefix, "goose");
        assert_eq!(b.deeplink_scheme, "goose");
        assert_eq!(b.github_owner, "aaif-goose");
        assert_eq!(b.github_repo, "goose");
        assert_eq!(b.agent_identity_sentence, "You are goose, an AI assistant.");
    }

    #[test]
    fn capitalize_ascii() {
        assert_eq!(capitalize("goose"), "Goose");
        assert_eq!(capitalize("foobar"), "Foobar");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("A"), "A");
    }

    #[test]
    fn apply_branding_is_noop_on_default_build() {
        use clap::CommandFactory;
        let before = crate::Cli::command().render_long_help().to_string();
        let after = branded_command().render_long_help().to_string();
        assert_eq!(before, after);
    }

    #[test]
    fn rebrand_str_rewrites_tokens() {
        let b = Brand {
            product_name: "Foobar",
            binary_name: "foobar",
            shell_alias_primary: "foobar",
            shell_alias_short: "fb",
            shell_fn_prefix: "foobar",
            deeplink_scheme: "foobar",
            github_owner: "acme",
            github_repo: "foobar",
            agent_identity_sentence: "You are foobar, an AI assistant.",
        };
        assert_eq!(
            rebrand_str("Configure goose settings", &b),
            "Configure foobar settings"
        );
        assert_eq!(
            rebrand_str("Check that your Goose setup is working", &b),
            "Check that your Foobar setup is working"
        );
        assert_eq!(
            rebrand_str("eval \"$(goose term init zsh)\"", &b),
            "eval \"$(foobar term init zsh)\""
        );
    }

    #[test]
    fn synthetic_non_default_brand_rewrites_help() {
        // Simulate a non-default build by walking a fresh command tree with
        // an explicit non-default brand.
        use clap::CommandFactory;
        let b = Brand {
            product_name: "Foobar",
            binary_name: "foobar",
            shell_alias_primary: "foobar",
            shell_alias_short: "fb",
            shell_fn_prefix: "foobar",
            deeplink_scheme: "foobar",
            github_owner: "acme",
            github_repo: "foobar",
            agent_identity_sentence: "You are foobar, an AI assistant.",
        };
        let mut cmd = rewrite_command(crate::Cli::command(), &b);
        assert_eq!(cmd.get_name(), "foobar");
        let rendered = cmd.render_long_help().to_string();
        assert!(!rendered.contains("goose"), "{rendered}");
        assert!(!rendered.contains("Goose"), "{rendered}");
    }
}
