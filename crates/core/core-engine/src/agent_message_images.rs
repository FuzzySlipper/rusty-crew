//! Crew-owned image attachment resolution for external agent deliveries.

use super::*;
use rusty_crew_core_protocol::ExternalTurnInputPart;

impl CoreEngine {
    pub(crate) fn external_image_inputs(
        &self,
        session_id: &SessionId,
        attachment_ids: &[String],
    ) -> CoreResult<Vec<ExternalTurnInputPart>> {
        const MAX_EXTERNAL_INPUT_IMAGES: usize = 4;
        const MAX_EXTERNAL_INPUT_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
        if attachment_ids.len() > MAX_EXTERNAL_INPUT_IMAGES {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "external_message_image_limit_exceeded",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        let available = self.store.query_chat_attachments(&AttachmentQuery {
            session_id: Some(session_id.clone()),
            include_removed: true,
            include_expired: true,
            ..AttachmentQuery::default()
        })?;
        attachment_ids
            .iter()
            .map(|attachment_id| {
                if !seen.insert(attachment_id.as_str()) {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "external_message_image_duplicate",
                    ));
                }
                let attachment = available
                    .iter()
                    .find(|candidate| candidate.attachment_id.0 == *attachment_id)
                    .ok_or_else(|| {
                        CoreError::new(CoreErrorKind::NotFound, "external_message_image_not_found")
                    })?;
                if attachment.status != AttachmentStatus::Active {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        "external_message_image_inactive",
                    ));
                }
                if !matches!(
                    attachment.mime_type.as_str(),
                    "image/png" | "image/jpeg" | "image/webp"
                ) {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "external_message_image_mime_unsupported",
                    ));
                }
                if attachment.byte_size == 0
                    || attachment.byte_size > MAX_EXTERNAL_INPUT_IMAGE_BYTES
                {
                    return Err(CoreError::new(
                        CoreErrorKind::InvalidInput,
                        "external_message_image_size_invalid",
                    ));
                }
                let storage_url = attachment.storage_url.clone().ok_or_else(|| {
                    CoreError::new(
                        CoreErrorKind::ActionRejected,
                        "external_message_image_content_unavailable",
                    )
                })?;
                if !storage_url.starts_with("artifact://tool-media/") {
                    return Err(CoreError::new(
                        CoreErrorKind::ActionRejected,
                        "external_message_image_content_unavailable",
                    ));
                }
                Ok(ExternalTurnInputPart::Image { url: storage_url })
            })
            .collect()
    }
}
