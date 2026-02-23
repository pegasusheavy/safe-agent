use std::collections::HashSet;

use serde::Serialize;

use llmfit_core::fit::{FitLevel, ModelFit, RunMode, rank_models_by_fit};
use llmfit_core::hardware::SystemSpecs;
use llmfit_core::models::{ModelDatabase, UseCase};
use llmfit_core::providers::{ModelProvider, OllamaProvider, ollama_pull_tag};

/// Serializable hardware summary returned by the `/api/llm/advisor/system` endpoint.
#[derive(Debug, Serialize)]
pub struct SystemReport {
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub cpu_cores: usize,
    pub cpu_name: String,
    pub has_gpu: bool,
    pub gpu_name: Option<String>,
    pub gpu_vram_gb: Option<f64>,
    pub gpu_count: u32,
    pub backend: String,
    pub unified_memory: bool,
}

/// A single model recommendation serialized for the dashboard.
#[derive(Debug, Serialize)]
pub struct ModelRecommendation {
    pub name: String,
    pub provider: String,
    pub params_b: f64,
    pub use_case: String,
    pub score: f64,
    pub fit_level: String,
    pub run_mode: String,
    pub best_quant: String,
    pub estimated_tps: f64,
    pub memory_required_gb: f64,
    pub ollama_tag: Option<String>,
    pub installed: bool,
}

/// Ollama provider status.
#[derive(Debug, Serialize)]
pub struct OllamaStatus {
    pub available: bool,
    pub installed_models: Vec<String>,
}

/// Detect system hardware using llmfit-core.
pub fn detect_system() -> SystemReport {
    let specs = SystemSpecs::detect();
    SystemReport {
        total_ram_gb: specs.total_ram_gb,
        available_ram_gb: specs.available_ram_gb,
        cpu_cores: specs.total_cpu_cores,
        cpu_name: specs.cpu_name.clone(),
        has_gpu: specs.has_gpu,
        gpu_name: specs.gpu_name.clone(),
        gpu_vram_gb: specs.gpu_vram_gb,
        gpu_count: specs.gpu_count,
        backend: format!("{:?}", specs.backend),
        unified_memory: specs.unified_memory,
    }
}

fn fit_level_str(level: &FitLevel) -> &'static str {
    match level {
        FitLevel::Perfect => "Perfect",
        FitLevel::Good => "Good",
        FitLevel::Marginal => "Marginal",
        FitLevel::TooTight => "TooTight",
    }
}

fn run_mode_str(mode: &RunMode) -> &'static str {
    match mode {
        RunMode::Gpu => "GPU",
        RunMode::MoeOffload => "MoE Offload",
        RunMode::CpuOffload => "CPU+GPU",
        RunMode::CpuOnly => "CPU",
    }
}

fn use_case_str(uc: &UseCase) -> &'static str {
    match uc {
        UseCase::General => "General",
        UseCase::Coding => "Coding",
        UseCase::Reasoning => "Reasoning",
        UseCase::Chat => "Chat",
        UseCase::Multimodal => "Multimodal",
        UseCase::Embedding => "Embedding",
    }
}

/// Get scored model recommendations for the current hardware.
///
/// `use_case_filter` narrows results to a specific category (or `None` for all).
/// `limit` caps the number of results (0 = unlimited).
pub fn recommend_models(
    use_case_filter: Option<&str>,
    limit: usize,
) -> Vec<ModelRecommendation> {
    let specs = SystemSpecs::detect();
    let db = ModelDatabase::new();

    let ollama = OllamaProvider::default();
    let installed: HashSet<String> = if ollama.is_available() {
        ollama.installed_models()
    } else {
        HashSet::new()
    };

    let filter_uc: Option<UseCase> = use_case_filter.and_then(|s| match s.to_lowercase().as_str() {
        "general" => Some(UseCase::General),
        "coding" => Some(UseCase::Coding),
        "reasoning" => Some(UseCase::Reasoning),
        "chat" => Some(UseCase::Chat),
        "multimodal" => Some(UseCase::Multimodal),
        "embedding" => Some(UseCase::Embedding),
        _ => None,
    });

    let mut fits: Vec<ModelFit> = db
        .get_all_models()
        .iter()
        .map(|m| {
            let mut fit = ModelFit::analyze(m, &specs);
            let tag = ollama_pull_tag(&m.name);
            if let Some(ref t) = tag {
                if installed.contains(t) {
                    fit.installed = true;
                }
            }
            fit
        })
        .filter(|f| f.fit_level != FitLevel::TooTight)
        .filter(|f| match filter_uc {
            Some(uc) => f.use_case == uc,
            None => true,
        })
        .collect();

    fits = rank_models_by_fit(fits);

    if limit > 0 {
        fits.truncate(limit);
    }

    fits.into_iter()
        .map(|f| {
            let tag = ollama_pull_tag(&f.model.name);
            ModelRecommendation {
                name: f.model.name.clone(),
                provider: f.model.provider.clone(),
                params_b: f.model.params_b(),
                use_case: use_case_str(&f.use_case).to_string(),
                score: f.score,
                fit_level: fit_level_str(&f.fit_level).to_string(),
                run_mode: run_mode_str(&f.run_mode).to_string(),
                best_quant: f.best_quant.clone(),
                estimated_tps: f.estimated_tps,
                memory_required_gb: f.memory_required_gb,
                ollama_tag: tag,
                installed: f.installed,
            }
        })
        .collect()
}

/// Check whether Ollama is reachable and list installed models.
pub fn check_ollama() -> OllamaStatus {
    let provider = OllamaProvider::default();
    let available = provider.is_available();
    let installed_models = if available {
        let mut models: Vec<String> = provider.installed_models().into_iter().collect();
        models.sort();
        models
    } else {
        Vec::new()
    };
    OllamaStatus {
        available,
        installed_models,
    }
}
