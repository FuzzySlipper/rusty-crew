//! PostgreSQL normalized model endpoint and configuration repositories.

use super::*;

pub(super) fn apply_postgres_model_registry(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    tx.batch_execute(&format!(
        "CREATE TABLE IF NOT EXISTS {schema}.model_endpoints (
            endpoint_id TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            protocol TEXT NOT NULL,
            credential_id TEXT,
            record_json TEXT NOT NULL,
            revision BIGINT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS model_endpoints_status_idx
            ON {schema}.model_endpoints(status, updated_at DESC, endpoint_id);
         CREATE INDEX IF NOT EXISTS model_endpoints_protocol_idx
            ON {schema}.model_endpoints(protocol, endpoint_id);
         CREATE TABLE IF NOT EXISTS {schema}.model_configurations (
            model_config_id TEXT PRIMARY KEY,
            endpoint_id TEXT NOT NULL REFERENCES {schema}.model_endpoints(endpoint_id),
            status TEXT NOT NULL,
            model_id TEXT NOT NULL,
            record_json TEXT NOT NULL,
            revision BIGINT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS model_configurations_endpoint_idx
            ON {schema}.model_configurations(endpoint_id, status, model_config_id);
         CREATE INDEX IF NOT EXISTS model_configurations_model_idx
            ON {schema}.model_configurations(model_id, model_config_id);"
    ))
    .map_err(|error| postgres_error("create PostgreSQL normalized model registries", error))?;
    backfill_postgres_legacy_model_registry(tx, schema)
}

impl PostgresBackendStore {
    pub fn upsert_model_endpoint(
        &self,
        write: &ModelEndpointWrite,
    ) -> CoreResult<ModelEndpointRecord> {
        write.validate()?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client
            .transaction()
            .map_err(|error| postgres_error("start PostgreSQL model endpoint upsert", error))?;
        validate_postgres_endpoint_credential(&mut tx, &schema, write)?;
        let existing = get_endpoint(&mut tx, &schema, &write.endpoint_id)?;
        validate_revision(
            "model endpoint",
            &write.endpoint_id,
            write.expected_revision,
            existing.as_ref().map(|record| record.revision),
        )?;
        let record = ModelEndpointRecord {
            endpoint_id: write.endpoint_id.clone(),
            status: write.status,
            display_name: write.display_name.clone(),
            description: write.description.clone(),
            base_url: write.base_url.clone(),
            protocol: write.protocol,
            wire_dialect: write.wire_dialect,
            auth_scheme: write.auth_scheme,
            credential_id: write.credential_id.clone(),
            prompt_cache_transport: write.prompt_cache_transport,
            metadata_json: write.metadata_json.clone(),
            revision: existing.as_ref().map_or(1, |record| record.revision + 1),
            created_at: existing
                .as_ref()
                .map_or_else(|| write.now.clone(), |record| record.created_at.clone()),
            updated_at: write.now.clone(),
        };
        let status = enum_text(record.status)?;
        let protocol = enum_text(record.protocol)?;
        let json = to_json_text(&record)?;
        let changed = if let Some(existing) = existing.as_ref() {
            tx.execute(
                &format!(
                    "UPDATE {schema}.model_endpoints
                     SET status=$2,protocol=$3,credential_id=$4,record_json=$5,revision=$6,updated_at=$7
                     WHERE endpoint_id=$1 AND revision=$8"
                ),
                &[&record.endpoint_id, &status, &protocol, &record.credential_id, &json,
                    &(record.revision as i64), &record.updated_at, &(existing.revision as i64)],
            )
        } else {
            tx.execute(
                &format!(
                    "INSERT INTO {schema}.model_endpoints
                     (endpoint_id,status,protocol,credential_id,record_json,revision,created_at,updated_at)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"
                ),
                &[&record.endpoint_id, &status, &protocol, &record.credential_id, &json,
                    &(record.revision as i64), &record.created_at, &record.updated_at],
            )
        }
        .map_err(|error| postgres_error("write PostgreSQL model endpoint", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!("model endpoint {} changed concurrently", record.endpoint_id),
            ));
        }
        sync_endpoint_shadows(&mut tx, &schema, &record)?;
        tx.commit()
            .map_err(|error| postgres_error("commit PostgreSQL model endpoint upsert", error))?;
        Ok(record)
    }

    pub fn get_model_endpoint(&self, endpoint_id: &str) -> CoreResult<Option<ModelEndpointRecord>> {
        ModelEndpointQuery {
            endpoint_id: Some(endpoint_id.to_string()),
            ..Default::default()
        }
        .validate()?;
        let schema = self.quoted_schema();
        get_endpoint(&mut *self.client()?, &schema, endpoint_id)
    }

    pub fn list_model_endpoints(
        &self,
        query: &ModelEndpointQuery,
    ) -> CoreResult<Vec<ModelEndpointRecord>> {
        query.validate()?;
        let schema = self.quoted_schema();
        let status = query.status.map(enum_text).transpose()?;
        let limit = query.limit.unwrap_or(100).clamp(1, 1_000) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        self.client()?
            .query(
                &format!(
                    "SELECT record_json FROM {schema}.model_endpoints
                     WHERE ($1::TEXT IS NULL OR endpoint_id=$1)
                       AND ($2::TEXT IS NULL OR status=$2)
                     ORDER BY updated_at DESC,endpoint_id ASC LIMIT $3 OFFSET $4"
                ),
                &[&query.endpoint_id, &status, &limit, &offset],
            )
            .map_err(|error| postgres_error("list PostgreSQL model endpoints", error))?
            .iter()
            .map(|row| parse_json(row.get::<_, String>(0), "model endpoint record_json"))
            .collect()
    }

    pub fn upsert_model_configuration(
        &self,
        write: &ModelConfigurationWrite,
    ) -> CoreResult<ModelConfigurationRecord> {
        write.validate()?;
        let schema = self.quoted_schema();
        let mut client = self.client()?;
        let mut tx = client.transaction().map_err(|error| {
            postgres_error("start PostgreSQL model configuration upsert", error)
        })?;
        let endpoint = get_endpoint(&mut tx, &schema, &write.endpoint_id)?.ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("model endpoint {} not found", write.endpoint_id),
            )
        })?;
        write.validate_for_endpoint(&endpoint)?;
        let existing = get_configuration(&mut tx, &schema, &write.model_config_id)?;
        validate_revision(
            "model configuration",
            &write.model_config_id,
            write.expected_revision,
            existing.as_ref().map(|record| record.revision),
        )?;
        let record = ModelConfigurationRecord {
            model_config_id: write.model_config_id.clone(),
            endpoint_id: write.endpoint_id.clone(),
            status: write.status,
            display_name: write.display_name.clone(),
            description: write.description.clone(),
            model_id: write.model_id.clone(),
            context_window_tokens: write.context_window_tokens,
            max_output_tokens: write.max_output_tokens,
            temperature_milli: write.temperature_milli,
            reasoning_effort: write.reasoning_effort.clone(),
            reasoning_format: write.reasoning_format.clone(),
            reasoning_history: write.reasoning_history,
            reasoning_budget_tokens: write.reasoning_budget_tokens,
            thinking_mode: write.thinking_mode,
            prompt_caching_policy: write.prompt_caching_policy,
            capabilities: write.capabilities.clone(),
            metadata_json: write.metadata_json.clone(),
            revision: existing.as_ref().map_or(1, |record| record.revision + 1),
            created_at: existing
                .as_ref()
                .map_or_else(|| write.now.clone(), |record| record.created_at.clone()),
            updated_at: write.now.clone(),
        };
        let status = enum_text(record.status)?;
        let json = to_json_text(&record)?;
        let changed = if let Some(existing) = existing.as_ref() {
            tx.execute(&format!(
                "UPDATE {schema}.model_configurations
                 SET endpoint_id=$2,status=$3,model_id=$4,record_json=$5,revision=$6,updated_at=$7
                 WHERE model_config_id=$1 AND revision=$8"),
                &[&record.model_config_id,&record.endpoint_id,&status,&record.model_id,&json,
                    &(record.revision as i64),&record.updated_at,&(existing.revision as i64)])
        } else {
            tx.execute(&format!(
                "INSERT INTO {schema}.model_configurations
                 (model_config_id,endpoint_id,status,model_id,record_json,revision,created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"),
                &[&record.model_config_id,&record.endpoint_id,&status,&record.model_id,&json,
                    &(record.revision as i64),&record.created_at,&record.updated_at])
        }.map_err(|error| postgres_error("write PostgreSQL model configuration", error))?;
        if changed != 1 {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!(
                    "model configuration {} changed concurrently",
                    record.model_config_id
                ),
            ));
        }
        sync_configuration_shadow(&mut tx, &schema, &endpoint, &record)?;
        tx.commit().map_err(|error| {
            postgres_error("commit PostgreSQL model configuration upsert", error)
        })?;
        Ok(record)
    }

    pub fn get_model_configuration(
        &self,
        model_config_id: &str,
    ) -> CoreResult<Option<ModelConfigurationRecord>> {
        ModelConfigurationQuery {
            model_config_id: Some(model_config_id.to_string()),
            ..Default::default()
        }
        .validate()?;
        let schema = self.quoted_schema();
        get_configuration(&mut *self.client()?, &schema, model_config_id)
    }

    pub fn list_model_configurations(
        &self,
        query: &ModelConfigurationQuery,
    ) -> CoreResult<Vec<ModelConfigurationRecord>> {
        query.validate()?;
        let schema = self.quoted_schema();
        let status = query.status.map(enum_text).transpose()?;
        let limit = query.limit.unwrap_or(100).clamp(1, 1_000) as i64;
        let offset = query.offset.unwrap_or(0) as i64;
        self.client()?
            .query(
                &format!(
                    "SELECT record_json FROM {schema}.model_configurations
             WHERE ($1::TEXT IS NULL OR model_config_id=$1)
               AND ($2::TEXT IS NULL OR endpoint_id=$2)
               AND ($3::TEXT IS NULL OR status=$3)
             ORDER BY updated_at DESC,model_config_id ASC LIMIT $4 OFFSET $5"
                ),
                &[
                    &query.model_config_id,
                    &query.endpoint_id,
                    &status,
                    &limit,
                    &offset,
                ],
            )
            .map_err(|error| postgres_error("list PostgreSQL model configurations", error))?
            .iter()
            .map(|row| parse_json(row.get::<_, String>(0), "model configuration record_json"))
            .collect()
    }
}

fn backfill_postgres_legacy_model_registry(
    tx: &mut Transaction<'_>,
    schema: &str,
) -> CoreResult<()> {
    let catalog_schema = schema
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(schema)
        .replace("\"\"", "\"")
        .chars()
        .take(63)
        .collect::<String>();
    let has_credentials: bool = tx
        .query_one(
            "SELECT EXISTS (
                SELECT 1
                FROM pg_catalog.pg_class c
                JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
                WHERE n.nspname::TEXT=$1 AND c.relname='service_credentials'
             )",
            &[&catalog_schema],
        )
        .map_err(|error| postgres_error("inspect PostgreSQL credential table for backfill", error))?
        .get(0);
    if !has_credentials {
        return Ok(());
    }
    let rows = tx
        .query(
            &format!(
                "SELECT mp.provider_json,mp.credential_id,sc.credential_kind,
                        sc.secret_ciphertext,sc.secret_updated_at,sc.revision
                 FROM {schema}.model_providers mp
                 LEFT JOIN {schema}.service_credentials sc ON sc.credential_id=mp.credential_id
                 ORDER BY mp.alias"
            ),
            &[],
        )
        .map_err(|error| {
            postgres_error("query PostgreSQL legacy model registry backfill", error)
        })?;
    for row in rows {
        let Ok(mut provider) = parse_json::<ModelProviderRecord>(
            row.get::<_, String>(0),
            "legacy model provider_json",
        ) else {
            continue;
        };
        provider.credential_id = row.get(1);
        let kind_raw: Option<String> = row.get(2);
        let secret: Option<String> = row.get(3);
        provider.credential = ModelProviderCredential {
            has_secret: secret.is_some(),
            secret_ref: secret
                .as_ref()
                .and(provider.credential_id.as_ref())
                .map(|id| format!("db://service_credentials/{id}/secret")),
            updated_at: row.get(4),
            kind: kind_raw.as_deref().map(parse_credential_kind).transpose()?,
            revision: row.get::<_, Option<i64>>(5).map(|value| value as u64),
        };
        let Ok((endpoint, configuration)) = normalized_from_provider(&provider) else {
            continue;
        };
        insert_endpoint_if_absent(tx, schema, &endpoint)?;
        insert_configuration_if_absent(tx, schema, &configuration)?;
    }
    Ok(())
}

fn parse_credential_kind(raw: &str) -> CoreResult<ModelProviderCredentialKind> {
    match raw {
        "api_key" => Ok(ModelProviderCredentialKind::ApiKey),
        "openai_oauth" => Ok(ModelProviderCredentialKind::OpenAiOauth),
        "legacy_raw_api_key" => Ok(ModelProviderCredentialKind::LegacyRawApiKey),
        _ => Err(CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("unknown credential kind {raw}"),
        )),
    }
}

fn sync_endpoint_shadows<C: GenericClient>(
    client: &mut C,
    schema: &str,
    endpoint: &ModelEndpointRecord,
) -> CoreResult<()> {
    let rows = client
        .query(
            &format!(
                "SELECT record_json FROM {schema}.model_configurations
                 WHERE endpoint_id=$1 ORDER BY model_config_id"
            ),
            &[&endpoint.endpoint_id],
        )
        .map_err(|error| postgres_error("query PostgreSQL endpoint shadows", error))?;
    for row in rows {
        let configuration = parse_json::<ModelConfigurationRecord>(
            row.get::<_, String>(0),
            "model configuration record_json",
        )?;
        sync_configuration_shadow(client, schema, endpoint, &configuration)?;
    }
    Ok(())
}

fn sync_configuration_shadow<C: GenericClient>(
    client: &mut C,
    schema: &str,
    endpoint: &ModelEndpointRecord,
    configuration: &ModelConfigurationRecord,
) -> CoreResult<()> {
    let existing = client
        .query_opt(
            &format!(
                "SELECT provider_json,revision,created_at FROM {schema}.model_providers WHERE alias=$1"
            ),
            &[&configuration.model_config_id],
        )
        .map_err(|error| postgres_error("load PostgreSQL provider shadow", error))?;
    let (revision, created_at) = existing.as_ref().map_or_else(
        || (1, configuration.created_at.clone()),
        |row| (row.get::<_, i64>(1) as u64 + 1, row.get::<_, String>(2)),
    );
    let credential =
        endpoint_credential_summary(client, schema, endpoint.credential_id.as_deref())?;
    let provider_kind = endpoint
        .metadata_json
        .get("legacyVendorLabel")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("custom")
        .to_string();
    let responses_dialect = match endpoint.wire_dialect {
        ModelEndpointWireDialect::OpenaiStateful => Some(ResponsesProviderDialect::OpenaiStateful),
        ModelEndpointWireDialect::OpenaiStateless => {
            Some(ResponsesProviderDialect::OpenaiStateless)
        }
        ModelEndpointWireDialect::GenericStateless => {
            Some(ResponsesProviderDialect::GenericStateless)
        }
        ModelEndpointWireDialect::Deepseek
            if endpoint.protocol == ModelProviderProtocol::Responses =>
        {
            Some(ResponsesProviderDialect::Deepseek)
        }
        ModelEndpointWireDialect::Meta => Some(ResponsesProviderDialect::Meta),
        _ => None,
    };
    let chat_completions_dialect = match endpoint.wire_dialect {
        ModelEndpointWireDialect::Kimi => {
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Kimi
        }
        ModelEndpointWireDialect::Glm => rusty_crew_core_protocol::ChatCompletionsWireDialect::Glm,
        ModelEndpointWireDialect::Qwen => {
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Qwen
        }
        ModelEndpointWireDialect::Deepseek => {
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Deepseek
        }
        _ => rusty_crew_core_protocol::ChatCompletionsWireDialect::Standard,
    };
    let provider = ModelProviderRecord {
        alias: configuration.model_config_id.clone(),
        status: configuration.status,
        protocol: endpoint.protocol,
        provider_kind,
        display_name: configuration.display_name.clone(),
        description: configuration.description.clone(),
        base_url: Some(endpoint.base_url.clone()),
        model_id: configuration.model_id.clone(),
        context_window_tokens: configuration.context_window_tokens,
        max_output_tokens: configuration.max_output_tokens,
        temperature_milli: configuration.temperature_milli,
        reasoning_effort: configuration.reasoning_effort.clone(),
        reasoning_format: configuration.reasoning_format.clone(),
        responses_dialect,
        chat_completions_dialect,
        thinking_mode: configuration.thinking_mode,
        reasoning_history: configuration.reasoning_history,
        reasoning_budget_tokens: configuration.reasoning_budget_tokens,
        prompt_caching: configuration.prompt_caching_policy,
        credential_id: endpoint.credential_id.clone(),
        credential,
        metadata_json: configuration.metadata_json.clone(),
        revision,
        created_at: created_at.clone(),
        updated_at: configuration.updated_at.clone(),
    };
    client.execute(&format!(
        "INSERT INTO {schema}.model_providers
         (alias,status,protocol,provider_json,secret_ciphertext,secret_updated_at,revision,created_at,updated_at,credential_id)
         VALUES ($1,$2,$3,$4,NULL,NULL,$5,$6,$7,$8)
         ON CONFLICT(alias) DO UPDATE SET status=excluded.status,protocol=excluded.protocol,
           provider_json=excluded.provider_json,secret_ciphertext=NULL,secret_updated_at=NULL,
           revision=excluded.revision,updated_at=excluded.updated_at,credential_id=excluded.credential_id"),
        &[&provider.alias,&enum_text(provider.status)?,&enum_text(provider.protocol)?,&to_json_text(&provider)?,
            &(revision as i64),&created_at,&provider.updated_at,&provider.credential_id])
        .map_err(|error| postgres_error("write PostgreSQL legacy provider shadow", error))?;
    Ok(())
}

fn endpoint_credential_summary<C: GenericClient>(
    client: &mut C,
    schema: &str,
    credential_id: Option<&str>,
) -> CoreResult<ModelProviderCredential> {
    let Some(id) = credential_id else {
        return Ok(ModelProviderCredential {
            has_secret: false,
            secret_ref: None,
            updated_at: None,
            kind: None,
            revision: None,
        });
    };
    let row = client
        .query_one(
            &format!(
                "SELECT credential_kind,secret_ciphertext,secret_updated_at,revision
         FROM {schema}.service_credentials WHERE credential_id=$1"
            ),
            &[&id],
        )
        .map_err(|error| postgres_error("load PostgreSQL endpoint credential summary", error))?;
    let kind_raw: String = row.get(0);
    let kind = match kind_raw.as_str() {
        "api_key" => ModelProviderCredentialKind::ApiKey,
        "openai_oauth" => ModelProviderCredentialKind::OpenAiOauth,
        "legacy_raw_api_key" => ModelProviderCredentialKind::LegacyRawApiKey,
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::PersistenceFailure,
                format!("unknown credential kind {kind_raw}"),
            ))
        }
    };
    let secret: Option<String> = row.get(1);
    Ok(ModelProviderCredential {
        has_secret: secret.is_some(),
        secret_ref: secret
            .as_ref()
            .map(|_| format!("db://service_credentials/{id}/secret")),
        updated_at: row.get(2),
        kind: Some(kind),
        revision: Some(row.get::<_, i64>(3) as u64),
    })
}

pub(super) fn sync_legacy_provider_to_normalized_in_tx(
    tx: &mut Transaction<'_>,
    schema: &str,
    provider: &ModelProviderRecord,
) -> CoreResult<()> {
    let existing_configuration = get_configuration(tx, schema, &provider.alias)?;
    let (mut endpoint, mut configuration) = match normalized_from_provider(provider) {
        Ok(records) => records,
        Err(error)
            if error.kind == CoreErrorKind::InvalidInput && existing_configuration.is_none() =>
        {
            // Keep incomplete legacy rollback rows writable. They remain absent from the
            // normalized registry until deterministic projection is representable.
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    if let Some(existing) = existing_configuration.as_ref() {
        configuration.endpoint_id = existing.endpoint_id.clone();
        endpoint.endpoint_id = existing.endpoint_id.clone();
    }
    let existing_endpoint = get_endpoint(tx, schema, &endpoint.endpoint_id)?;
    if let Some(existing) = existing_endpoint.as_ref() {
        let shared_count: i64 = tx
            .query_one(
                &format!("SELECT COUNT(*) FROM {schema}.model_configurations WHERE endpoint_id=$1"),
                &[&endpoint.endpoint_id],
            )
            .map_err(|error| postgres_error("count PostgreSQL shared endpoint configs", error))?
            .get(0);
        if shared_count > 1 && endpoint_transport_changed(existing, &endpoint) {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                "legacy_provider_shared_endpoint_conflict",
            ));
        }
        endpoint.revision = existing.revision + 1;
        endpoint.created_at = existing.created_at.clone();
        update_endpoint_record(tx, schema, &endpoint, existing.revision)?;
    } else {
        insert_endpoint_if_absent(tx, schema, &endpoint)?;
    }
    if let Some(existing) = existing_configuration.as_ref() {
        configuration.revision = existing.revision + 1;
        configuration.created_at = existing.created_at.clone();
        update_configuration_record(tx, schema, &configuration, existing.revision)?;
    } else {
        insert_configuration_if_absent(tx, schema, &configuration)?;
    }
    Ok(())
}

fn normalized_from_provider(
    provider: &ModelProviderRecord,
) -> CoreResult<(ModelEndpointRecord, ModelConfigurationRecord)> {
    let base_url = provider
        .base_url
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "legacy base_url is not representable",
            )
        })?;
    let wire_dialect = match provider.protocol {
        ModelProviderProtocol::Responses => match provider.responses_dialect {
            Some(ResponsesProviderDialect::OpenaiStateful) => {
                ModelEndpointWireDialect::OpenaiStateful
            }
            Some(ResponsesProviderDialect::OpenaiStateless) => {
                ModelEndpointWireDialect::OpenaiStateless
            }
            Some(ResponsesProviderDialect::GenericStateless) => {
                ModelEndpointWireDialect::GenericStateless
            }
            Some(ResponsesProviderDialect::Deepseek) => ModelEndpointWireDialect::Deepseek,
            Some(ResponsesProviderDialect::Meta) => ModelEndpointWireDialect::Meta,
            None => {
                return Err(CoreError::new(
                    CoreErrorKind::InvalidInput,
                    "legacy responses_dialect is not representable",
                ))
            }
        },
        ModelProviderProtocol::ChatCompletions => match provider.chat_completions_dialect {
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Standard => {
                ModelEndpointWireDialect::Standard
            }
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Kimi => {
                ModelEndpointWireDialect::Kimi
            }
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Glm => {
                ModelEndpointWireDialect::Glm
            }
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Qwen => {
                ModelEndpointWireDialect::Qwen
            }
            rusty_crew_core_protocol::ChatCompletionsWireDialect::Deepseek => {
                ModelEndpointWireDialect::Deepseek
            }
        },
    };
    let auth_scheme = match provider.credential.kind {
        None if provider.credential_id.is_none() => ModelEndpointAuthScheme::None,
        Some(
            ModelProviderCredentialKind::ApiKey | ModelProviderCredentialKind::LegacyRawApiKey,
        ) => ModelEndpointAuthScheme::BearerApiKey,
        Some(ModelProviderCredentialKind::OpenAiOauth)
            if provider.protocol == ModelProviderProtocol::Responses =>
        {
            ModelEndpointAuthScheme::OpenAiCodexOauth
        }
        _ => {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "legacy credential binding is not representable",
            ))
        }
    };
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "legacyVendorLabel".to_string(),
        serde_json::Value::String(provider.provider_kind.clone()),
    );
    let endpoint = ModelEndpointRecord {
        endpoint_id: provider.alias.clone(),
        status: provider.status,
        display_name: provider.display_name.clone(),
        description: provider.description.clone(),
        base_url,
        protocol: provider.protocol,
        wire_dialect,
        auth_scheme,
        credential_id: provider.credential_id.clone(),
        prompt_cache_transport: if provider.prompt_caching
            == rusty_crew_core_protocol::ChatCompletionsPromptCachingPolicy::Disabled
        {
            PromptCacheTransport::None
        } else {
            PromptCacheTransport::OpenrouterAnthropic
        },
        metadata_json: serde_json::Value::Object(metadata),
        revision: provider.revision,
        created_at: provider.created_at.clone(),
        updated_at: provider.updated_at.clone(),
    };
    endpoint.validate()?;
    let configuration = ModelConfigurationRecord {
        model_config_id: provider.alias.clone(),
        endpoint_id: provider.alias.clone(),
        status: provider.status,
        display_name: provider.display_name.clone(),
        description: provider.description.clone(),
        model_id: provider.model_id.clone(),
        context_window_tokens: provider.context_window_tokens,
        max_output_tokens: provider.max_output_tokens,
        temperature_milli: provider.temperature_milli,
        reasoning_effort: provider.reasoning_effort.clone(),
        reasoning_format: provider.reasoning_format.clone(),
        reasoning_history: provider.reasoning_history,
        reasoning_budget_tokens: provider.reasoning_budget_tokens,
        thinking_mode: provider.thinking_mode,
        prompt_caching_policy: provider.prompt_caching,
        capabilities: Default::default(),
        metadata_json: provider.metadata_json.clone(),
        revision: provider.revision,
        created_at: provider.created_at.clone(),
        updated_at: provider.updated_at.clone(),
    };
    configuration.validate_for_endpoint(&endpoint)?;
    Ok((endpoint, configuration))
}

fn endpoint_transport_changed(a: &ModelEndpointRecord, b: &ModelEndpointRecord) -> bool {
    a.base_url != b.base_url
        || a.protocol != b.protocol
        || a.wire_dialect != b.wire_dialect
        || a.auth_scheme != b.auth_scheme
        || a.credential_id != b.credential_id
        || a.prompt_cache_transport != b.prompt_cache_transport
}

fn insert_endpoint_if_absent<C: GenericClient>(
    client: &mut C,
    schema: &str,
    record: &ModelEndpointRecord,
) -> CoreResult<()> {
    let status = enum_text(record.status)?;
    let protocol = enum_text(record.protocol)?;
    client
        .execute(
            &format!(
                "INSERT INTO {schema}.model_endpoints
                 (endpoint_id,status,protocol,credential_id,record_json,revision,created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(endpoint_id) DO NOTHING"
            ),
            &[
                &record.endpoint_id,
                &status,
                &protocol,
                &record.credential_id,
                &to_json_text(record)?,
                &(record.revision as i64),
                &record.created_at,
                &record.updated_at,
            ],
        )
        .map_err(|error| postgres_error("backfill PostgreSQL model endpoint", error))?;
    Ok(())
}

fn insert_configuration_if_absent<C: GenericClient>(
    client: &mut C,
    schema: &str,
    record: &ModelConfigurationRecord,
) -> CoreResult<()> {
    let status = enum_text(record.status)?;
    client
        .execute(
            &format!(
                "INSERT INTO {schema}.model_configurations
                 (model_config_id,endpoint_id,status,model_id,record_json,revision,created_at,updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(model_config_id) DO NOTHING"
            ),
            &[
                &record.model_config_id,
                &record.endpoint_id,
                &status,
                &record.model_id,
                &to_json_text(record)?,
                &(record.revision as i64),
                &record.created_at,
                &record.updated_at,
            ],
        )
        .map_err(|error| postgres_error("backfill PostgreSQL model configuration", error))?;
    Ok(())
}

fn update_endpoint_record<C: GenericClient>(
    client: &mut C,
    schema: &str,
    record: &ModelEndpointRecord,
    expected: u64,
) -> CoreResult<()> {
    let changed = client
        .execute(
            &format!(
                "UPDATE {schema}.model_endpoints SET status=$2,protocol=$3,credential_id=$4,
                 record_json=$5,revision=$6,updated_at=$7 WHERE endpoint_id=$1 AND revision=$8"
            ),
            &[
                &record.endpoint_id,
                &enum_text(record.status)?,
                &enum_text(record.protocol)?,
                &record.credential_id,
                &to_json_text(record)?,
                &(record.revision as i64),
                &record.updated_at,
                &(expected as i64),
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL normalized endpoint", error))?;
    if changed != 1 {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("model endpoint {} changed concurrently", record.endpoint_id),
        ));
    }
    Ok(())
}

fn update_configuration_record<C: GenericClient>(
    client: &mut C,
    schema: &str,
    record: &ModelConfigurationRecord,
    expected: u64,
) -> CoreResult<()> {
    let changed = client
        .execute(
            &format!(
                "UPDATE {schema}.model_configurations SET endpoint_id=$2,status=$3,model_id=$4,
         record_json=$5,revision=$6,updated_at=$7 WHERE model_config_id=$1 AND revision=$8"
            ),
            &[
                &record.model_config_id,
                &record.endpoint_id,
                &enum_text(record.status)?,
                &record.model_id,
                &to_json_text(record)?,
                &(record.revision as i64),
                &record.updated_at,
                &(expected as i64),
            ],
        )
        .map_err(|error| postgres_error("update PostgreSQL normalized configuration", error))?;
    if changed != 1 {
        return Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!(
                "model configuration {} changed concurrently",
                record.model_config_id
            ),
        ));
    }
    Ok(())
}

fn get_endpoint<C: GenericClient>(
    client: &mut C,
    schema: &str,
    id: &str,
) -> CoreResult<Option<ModelEndpointRecord>> {
    client
        .query_opt(
            &format!("SELECT record_json FROM {schema}.model_endpoints WHERE endpoint_id=$1"),
            &[&id],
        )
        .map_err(|error| postgres_error("get PostgreSQL model endpoint", error))?
        .map(|row| parse_json(row.get::<_, String>(0), "model endpoint record_json"))
        .transpose()
}

fn get_configuration<C: GenericClient>(
    client: &mut C,
    schema: &str,
    id: &str,
) -> CoreResult<Option<ModelConfigurationRecord>> {
    client
        .query_opt(
            &format!(
                "SELECT record_json FROM {schema}.model_configurations WHERE model_config_id=$1"
            ),
            &[&id],
        )
        .map_err(|error| postgres_error("get PostgreSQL model configuration", error))?
        .map(|row| parse_json(row.get::<_, String>(0), "model configuration record_json"))
        .transpose()
}

fn validate_postgres_endpoint_credential<C: GenericClient>(
    client: &mut C,
    schema: &str,
    write: &ModelEndpointWrite,
) -> CoreResult<()> {
    let Some(id) = write.credential_id.as_deref() else {
        return Ok(());
    };
    let kind = client
        .query_opt(
            &format!(
                "SELECT credential_kind FROM {schema}.service_credentials WHERE credential_id=$1"
            ),
            &[&id],
        )
        .map_err(|error| postgres_error("load PostgreSQL endpoint credential", error))?
        .map(|row| row.get::<_, String>(0))
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("service credential {id} not found"),
            )
        })?;
    if matches!(
        (write.auth_scheme, kind.as_str()),
        (
            ModelEndpointAuthScheme::BearerApiKey,
            "api_key" | "legacy_raw_api_key"
        ) | (ModelEndpointAuthScheme::OpenAiCodexOauth, "openai_oauth")
    ) {
        Ok(())
    } else {
        Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!(
                "model endpoint {} auth scheme is incompatible with credential kind {kind}",
                write.endpoint_id
            ),
        ))
    }
}

fn validate_revision(
    kind: &str,
    id: &str,
    expected: Option<u64>,
    found: Option<u64>,
) -> CoreResult<()> {
    if let Some(expected) = expected {
        let found = found.unwrap_or(0);
        if expected != found {
            return Err(CoreError::new(
                CoreErrorKind::ActionRejected,
                format!("{kind} {id} revision mismatch: expected {expected}, found {found}"),
            ));
        }
    }
    Ok(())
}

fn enum_text<T: serde::Serialize>(value: T) -> CoreResult<String> {
    serde_json::to_value(value)
        .map_err(|error| CoreError::new(CoreErrorKind::InternalError, error.to_string()))?
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InternalError,
                "enum did not serialize as text",
            )
        })
}

fn parse_json<T: serde::de::DeserializeOwned>(value: String, label: &str) -> CoreResult<T> {
    serde_json::from_str(&value).map_err(|error| {
        CoreError::new(
            CoreErrorKind::PersistenceFailure,
            format!("parse {label}: {error}"),
        )
    })
}
