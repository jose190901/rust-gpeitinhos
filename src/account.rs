use serde_json::Value;

/// Estado mínimo que autoriza a rota de assinatura. Não contém tokens.
#[derive(Debug, Clone)]
pub struct SubscriptionAccount {
    pub plan: String,
}

impl SubscriptionAccount {
    pub fn verify(result: &Value) -> Result<Self, String> {
        let account = &result["account"];
        if account["type"].as_str() != Some("chatgpt") {
            return Err(
                "Entre com sua conta ChatGPT para usar a assinatura. A rota de API está bloqueada."
                    .into(),
            );
        }
        let plan = account["planType"]
            .as_str()
            .ok_or("O Codex não confirmou o plano da conta.")?;
        if !matches!(
            plan,
            "plus" | "pro" | "business" | "enterprise" | "edu" | "go" | "team"
        ) {
            return Err(
                "O plano retornado precisa ser verificado antes de enviar mensagens.".into(),
            );
        }
        Ok(Self {
            plan: plan.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_api_key_even_with_paid_plan_label() {
        assert!(
            SubscriptionAccount::verify(&json!({"account":{"type":"apiKey","planType":"plus"}}))
                .is_err()
        );
    }

    #[test]
    fn rejects_unknown_or_missing_account_and_plan() {
        for response in [
            json!({"account":null}),
            json!({"account":{"type":"chatgpt"}}),
            json!({"account":{"type":"chatgpt","planType":"unknown"}}),
        ] {
            assert!(SubscriptionAccount::verify(&response).is_err());
        }
    }

    #[test]
    fn supports_subscription_upgrade_without_api_fallback() {
        for plan in ["plus", "pro", "business", "enterprise", "edu"] {
            assert_eq!(
                SubscriptionAccount::verify(&json!({"account":{"type":"chatgpt","planType":plan}}))
                    .unwrap()
                    .plan,
                plan
            );
        }
    }
}
