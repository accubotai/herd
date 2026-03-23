use std::path::Path;

/// Detected framework/language for a project directory
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Framework {
    Laravel,
    NodeJs,
    NextJs,
    Nuxt,
    Django,
    Flask,
    FastApi,
    Rails,
    Go,
    Rust,
    DotNet,
    Elixir,
    Spring,
    Svelte,
    Remix,
    Astro,
    Unknown,
}

/// Detect the primary framework in a project directory
pub fn detect_framework(dir: &Path) -> Framework {
    // Check most specific first
    if dir.join("artisan").exists() {
        return Framework::Laravel;
    }
    if dir.join("next.config.js").exists()
        || dir.join("next.config.mjs").exists()
        || dir.join("next.config.ts").exists()
    {
        return Framework::NextJs;
    }
    if dir.join("nuxt.config.ts").exists() || dir.join("nuxt.config.js").exists() {
        return Framework::Nuxt;
    }
    if dir.join("svelte.config.js").exists() || dir.join("svelte.config.ts").exists() {
        return Framework::Svelte;
    }
    if dir.join("remix.config.js").exists() || dir.join("remix.config.ts").exists() {
        return Framework::Remix;
    }
    if dir.join("astro.config.mjs").exists() || dir.join("astro.config.ts").exists() {
        return Framework::Astro;
    }
    if dir.join("manage.py").exists() {
        // Could be Django, Flask, or FastAPI — check for Django settings
        if dir.join("settings.py").exists() || has_dir(dir, "*/settings.py") {
            return Framework::Django;
        }
    }
    if dir.join("Gemfile").exists() && dir.join("config.ru").exists() {
        return Framework::Rails;
    }
    if dir.join("mix.exs").exists() {
        return Framework::Elixir;
    }
    if dir.join("pom.xml").exists() || dir.join("build.gradle").exists() {
        return Framework::Spring;
    }
    if dir.join("go.mod").exists() {
        return Framework::Go;
    }
    if dir.join("Cargo.toml").exists() {
        return Framework::Rust;
    }
    if dir.join("package.json").exists() {
        return Framework::NodeJs;
    }
    if has_csproj(dir) {
        return Framework::DotNet;
    }
    if dir.join("requirements.txt").exists() || dir.join("pyproject.toml").exists() {
        // Generic Python — could be Flask/FastAPI
        return Framework::Flask; // default to Flask for generic Python
    }

    Framework::Unknown
}

/// Suggest default processes based on detected framework
pub fn suggest_processes(framework: &Framework) -> Vec<(&'static str, &'static str, &'static str)> {
    // Returns (name, command, section)
    match framework {
        Framework::Laravel => vec![
            ("Dev Server", "php artisan serve", "services"),
            ("Vite", "npm run dev", "services"),
            ("Queue Worker", "php artisan queue:work", "services"),
            ("Logs", "tail -f storage/logs/laravel.log", "services"),
        ],
        Framework::NextJs | Framework::NodeJs => {
            vec![("Dev Server", "npm run dev", "services")]
        }
        Framework::Django => vec![("Dev Server", "python manage.py runserver", "services")],
        Framework::Go => vec![("Run", "go run .", "services")],
        Framework::Rust => vec![
            ("Run", "cargo run", "services"),
            ("Watch", "cargo watch -x run", "services"),
        ],
        _ => vec![],
    }
}

fn has_dir(dir: &Path, _pattern: &str) -> bool {
    // Simplified: just check if common Django patterns exist
    dir.join("django").exists() || dir.join("wsgi.py").exists()
}

fn has_csproj(dir: &Path) -> bool {
    dir.read_dir().ok().is_some_and(|entries| {
        entries.filter_map(std::result::Result::ok).any(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "csproj" || ext == "sln")
        })
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_detect_nodejs() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(detect_framework(dir.path()), Framework::NodeJs);
    }

    #[test]
    fn test_detect_rust() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        assert_eq!(detect_framework(dir.path()), Framework::Rust);
    }

    #[test]
    fn test_detect_nextjs_over_nodejs() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("next.config.js"), "").unwrap();
        assert_eq!(detect_framework(dir.path()), Framework::NextJs);
    }

    #[test]
    fn test_detect_unknown() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_framework(dir.path()), Framework::Unknown);
    }

    #[test]
    fn test_suggest_processes_laravel() {
        let suggestions = suggest_processes(&Framework::Laravel);
        assert!(suggestions.len() >= 3);
        assert_eq!(suggestions[0].0, "Dev Server");
    }
}
