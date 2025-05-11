use serde::Deserialize;

#[derive(Deserialize, Default)]
pub struct XfoilResult {
    alpha: Vec<f64>,

    #[serde(rename = "CL")]
    cl: Vec<f64>,
    #[serde(rename = "CD")]
    cd: Vec<f64>,
    #[serde(rename = "CDp")]
    cd_p: Vec<f64>,
    #[serde(rename = "CM")]
    cm: Vec<f64>,
    #[serde(rename = "Top_Xtr")]
    top_xtr: Vec<f64>,
    #[serde(rename = "Bot_Xtr")]
    bot_xtr: Vec<f64>,
}

impl XfoilResult {
    pub fn get_analysis_result(&self, aoa: f64) -> AnalysisResult {
        if let Some(idx) = self.alpha.iter().position(|&x| x == aoa) {
            let cl = self.cl.get(idx).copied().expect("cl not found!");
            let cd = self.cd.get(idx).copied().expect("cd not found!");
            AnalysisResult::valid_result(aoa, cl, cd)
        } else {
            AnalysisResult::default()
        }
    }
    pub fn export(&self) -> Vec<AnalysisResult> {
        self.alpha
            .iter()
            .map(|&aoa| self.get_analysis_result(aoa))
            .collect()
    }
}

#[derive(Default)]
pub struct AnalysisResult {
    pub aoa: f64,
    pub cl: f64,
    pub cd: f64,
    pub ld_ratio: f64,
}

impl AnalysisResult {
    fn valid_result(aoa: f64, cl: f64, cd: f64) -> Self {
        let ld_ratio = if cd.abs() < 1e-9 { 0.0 } else { cl / cd }; // Avoid division by zero
        AnalysisResult {
            aoa,
            cl,
            cd,
            ld_ratio,
        }
    }
}
