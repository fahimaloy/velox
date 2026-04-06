//! Component Resolver for .vx imports
//!
//! This module handles importing and resolving .vx components
//! from other files, enabling component composition.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::sfc::Sfc;
use crate::template_ast::Node;

/// Represents an imported component
#[derive(Debug, Clone)]
pub struct ComponentImport {
    /// The local name used in the template (e.g., "MyButton")
    pub local_name: String,
    /// The source file path (e.g., "./components/Button.vx")
    pub source: String,
    /// Whether this is a default import
    pub default: bool,
    /// The parsed SFC (loaded lazily)
    pub sfc: Option<Sfc>,
}

/// Resolver for tracking and loading component imports
#[derive(Debug, Default)]
pub struct ComponentResolver {
    /// Map of local names to imports
    imports: HashMap<String, ComponentImport>,
    /// Base path for resolving relative imports
    base_path: PathBuf,
}

impl ComponentResolver {
    /// Create a new resolver with a base path for resolving imports
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            imports: HashMap::new(),
            base_path: base_path.into(),
        }
    }

    /// Parse import statements from script content
    pub fn parse_imports(&mut self, script_content: &str) {
        for line in script_content.lines() {
            let line = line.trim();

            // Parse: import MyButton from './components/Button.vx'
            if let Some(rest) = line.strip_prefix("import ")
                && let Some((import_part, source)) = rest.split_once(" from ")
            {
                let source = source.trim().trim_matches('"').trim_matches('\'');

                // Handle single default import
                let name = import_part.trim().to_string();
                self.add_import(name, source, true);
            }

            // Parse: import { Button, Card } from './components.vx'
            if let Some(rest) = line.strip_prefix("import {")
                && let Some((names, source)) = rest.split_once("} from ")
            {
                let source = source.trim().trim_matches('"').trim_matches('\'');
                for name in names.split(',') {
                    let name = name.trim();
                    if !name.is_empty() {
                        self.add_import(name.to_string(), source, false);
                    }
                }
            }
        }
    }

    /// Add an import to the resolver
    fn add_import(&mut self, local_name: String, source: &str, default: bool) {
        let import = ComponentImport {
            local_name: local_name.clone(),
            source: source.to_string(),
            default,
            sfc: None,
        };
        self.imports.insert(local_name, import);
    }

    /// Check if a tag name is a component import
    pub fn is_component(&self, tag: &str) -> bool {
        self.imports.contains_key(tag)
    }

    /// Get the import for a component name
    pub fn get_import(&self, name: &str) -> Option<&ComponentImport> {
        self.imports.get(name)
    }

    /// Load a component's SFC from disk
    pub fn load_component(&mut self, name: &str) -> Result<&Sfc, String> {
        if let Some(import) = self.imports.get(name) {
            if import.sfc.is_none() {
                let source_path = if Path::new(&import.source).is_absolute() {
                    PathBuf::from(&import.source)
                } else {
                    self.base_path.join(&import.source)
                };

                let content = std::fs::read_to_string(&source_path)
                    .map_err(|e| format!("Failed to read {}: {}", source_path.display(), e))?;

                let sfc = crate::parse_sfc(&content)?;

                if let Some(import) = self.imports.get_mut(name) {
                    import.sfc = Some(sfc);
                }
            }

            return self
                .imports
                .get(name)
                .and_then(|i| i.sfc.as_ref())
                .ok_or_else(|| format!("Component {} not loaded", name));
        }

        Err(format!("Component {} not found in imports", name))
    }

    /// Get all imported component names
    pub fn component_names(&self) -> Vec<String> {
        self.imports.keys().cloned().collect()
    }

    /// Resolve the full path for a component source
    pub fn resolve_path(&self, source: &str) -> PathBuf {
        let path = Path::new(source);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_path.join(path)
        }
    }
}

/// Transform a template AST to resolve component tags
/// This converts custom component tags to a standard representation
pub fn transform_components(nodes: &mut [Node], resolver: &ComponentResolver) {
    for node in nodes.iter_mut() {
        if let Node::Element {
            tag,
            attrs,
            children,
            ..
        } = node
        {
            // Check if this is a component
            if resolver.is_component(tag) {
                // Add a special marker attribute
                attrs.push(crate::template_ast::TemplateAttr {
                    name: String::from("data-velox-component"),
                    value: Some(tag.clone()),
                    kind: crate::template_ast::AttrKind::Static,
                });

                // Rename tag to div with component marker
                // The actual component will be rendered separately
                *tag = String::from("div");
            }

            // Recursively transform children
            transform_components(children, resolver);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_default_import() {
        let mut resolver = ComponentResolver::new("/tmp");
        resolver.parse_imports(r#"import MyButton from './components/Button.vx'"#);

        assert!(resolver.is_component("MyButton"));
        let import = resolver.get_import("MyButton").unwrap();
        assert_eq!(import.source, "./components/Button.vx");
        assert!(import.default);
    }

    #[test]
    fn test_parse_named_imports() {
        let mut resolver = ComponentResolver::new("/tmp");
        resolver.parse_imports(r#"import { Button, Card } from './components.vx'"#);

        assert!(resolver.is_component("Button"));
        assert!(resolver.is_component("Card"));
    }

    #[test]
    fn test_resolve_path() {
        let resolver = ComponentResolver::new("/home/project/src");

        let relative = resolver.resolve_path("./components/Button.vx");
        assert_eq!(
            relative,
            PathBuf::from("/home/project/src/components/Button.vx")
        );

        let absolute = resolver.resolve_path("/usr/lib/components.vx");
        assert_eq!(absolute, PathBuf::from("/usr/lib/components.vx"));
    }
}
