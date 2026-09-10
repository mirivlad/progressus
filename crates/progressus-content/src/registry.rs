//! The shared shape of every content registry.
//!
//! A definition carries a stable lowercase `name`. That name is the identity
//! that saves store and that localization keys off; the runtime handle is a
//! small index into the registry and is never persisted. Named constants are
//! resolved from the name at compile time, so a misspelling fails the build.

/// `str` equality usable in a `const fn`, which `==` is not.
pub const fn name_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// Declares a content handle over `$registry`, plus the `const fn` that
/// resolves a stable name to it.
macro_rules! content_handle {
    ($handle:ident, $definition:ident, $registry:ident, $lookup:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $handle(u16);

        impl $handle {
            /// The definition this handle addresses.
            pub const fn definition(self) -> &'static $definition {
                &$registry[self.0 as usize]
            }

            /// The stable name saves and localization use.
            pub const fn name(self) -> &'static str {
                self.definition().name
            }

            /// Resolves a persisted name. `None` means this build has no such
            /// definition, which callers must surface rather than substitute.
            pub fn from_name(name: &str) -> Option<Self> {
                $registry
                    .iter()
                    .position(|definition| definition.name == name)
                    .map(|index| Self(index as u16))
            }

            /// Every defined handle, in registry order.
            pub fn all() -> impl ExactSizeIterator<Item = Self> + Clone {
                (0..$registry.len()).map(|index| Self(index as u16))
            }

            /// How many definitions exist, for registry tests.
            pub const fn count() -> usize {
                $registry.len()
            }
        }

        /// Resolves a name at compile time. An unknown name fails the build.
        pub const fn $lookup(name: &str) -> $handle {
            let mut index = 0;
            while index < $registry.len() {
                if $crate::registry::name_eq($registry[index].name, name) {
                    return $handle(index as u16);
                }
                index += 1;
            }
            panic!("no content definition has this name");
        }
    };
}

pub(crate) use content_handle;

#[cfg(test)]
mod tests {
    use super::name_eq;

    #[test]
    fn const_name_equality_matches_ordinary_equality() {
        for (left, right) in [
            ("wood", "wood"),
            ("wood", "stone"),
            ("wood", "woods"),
            ("", ""),
            ("stone_wall", "stone_walL"),
        ] {
            assert_eq!(name_eq(left, right), left == right, "{left} vs {right}");
        }
    }
}
