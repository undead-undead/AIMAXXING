use std::collections::HashMap;

/// Pricing per 1M tokens in USD
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelPricing {
    pub prompt_price: f64,
    pub completion_price: f64,
}

pub struct PricingRegistry {
    prices: HashMap<String, ModelPricing>,
}

impl Default for PricingRegistry {
    fn default() -> Self {
        let mut prices = HashMap::new();
        // OpenAI
        prices.insert(
            "gpt-4o".to_string(),
            ModelPricing {
                prompt_price: 5.0,
                completion_price: 15.0,
            },
        );
        prices.insert(
            "gpt-4o-mini".to_string(),
            ModelPricing {
                prompt_price: 0.15,
                completion_price: 0.60,
            },
        );
        // DeepSeek
        prices.insert(
            "deepseek-chat".to_string(),
            ModelPricing {
                prompt_price: 0.07,
                completion_price: 1.10,
            },
        );
        prices.insert(
            "deepseek-reasoner".to_string(),
            ModelPricing {
                prompt_price: 0.55,
                completion_price: 2.19,
            },
        );
        // Anthropic
        prices.insert(
            "claude-3-5-sonnet-latest".to_string(),
            ModelPricing {
                prompt_price: 3.0,
                completion_price: 15.0,
            },
        );
        prices.insert(
            "claude-3-5-haiku-latest".to_string(),
            ModelPricing {
                prompt_price: 0.25,
                completion_price: 1.25,
            },
        );

        Self { prices }
    }
}

impl PricingRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_price(&self, model: &str) -> Option<&ModelPricing> {
        self.prices.get(model)
    }

    pub fn calculate_cost(&self, model: &str, prompt_tokens: u32, completion_tokens: u32) -> f64 {
        if let Some(p) = self.get_price(model) {
            (prompt_tokens as f64 / 1_000_000.0 * p.prompt_price)
                + (completion_tokens as f64 / 1_000_000.0 * p.completion_price)
        } else {
            0.0
        }
    }
}
