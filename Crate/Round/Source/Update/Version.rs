//! Parsing and comparing semantic version tags from GitHub releases.

/// Strip common release tag prefixes (`Radius/`, `v`, `V`).
pub fn Clean(Tag:&str) -> String {
	let Trimmed = Tag.trim();

	let WithoutScope = Trimmed
		.strip_prefix("Radius/")
		.or_else(|| Trimmed.strip_prefix("radius/"))
		.unwrap_or(Trimmed);

	WithoutScope
		.strip_prefix('v')
		.or_else(|| WithoutScope.strip_prefix('V'))
		.unwrap_or(WithoutScope)
		.to_owned()
}

/// Parse `major.minor.patch` from a tag string.
pub fn Parse(Tag:&str) -> Option<(u64, u64, u64)> {
	let Normalized = Clean(Tag);

	let Core = Normalized.split('-').next().unwrap_or(&Normalized);

	let mut Parts = Core.split('.');

	let Major = Parts.next()?.parse::<u64>().ok()?;

	let Minor = Parts.next().unwrap_or("0").parse::<u64>().ok()?;

	let Patch = Parts.next().unwrap_or("0").parse::<u64>().ok()?;

	Some((Major, Minor, Patch))
}

/// Returns `true` when `Latest` represents a strictly higher semver than `Current`.
pub fn IsNewer(Current:&str, Latest:&str) -> bool {
	matches!((Parse(Current), Parse(Latest)), (Some(Now), Some(Next)) if Next > Now)
}
