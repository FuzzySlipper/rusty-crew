use super::*;

#[test]
fn memory_space_catalog_uses_current_surface_boundaries() {
    let engine = test_engine();
    let descriptors = engine
        .list_memory_space_descriptors()
        .expect("list memory-space descriptors");

    assert_eq!(
        descriptors
            .iter()
            .map(|descriptor| descriptor.space_id.as_str())
            .collect::<Vec<_>>(),
        vec!["profile_dense", "session_memory", "roleplay_lore"]
    );

    let profile = &descriptors[0];
    assert!(profile
        .description
        .contains("Crew-owned profile/user memory"));
    assert!(profile.description.contains("external memory"));

    let session = &descriptors[1];
    assert!(session.description.contains("Branch-aware Crew-owned"));
    assert!(session.description.contains("prompt-selection diagnostics"));
    assert!(session.description.contains("runtime search"));

    let lore = &descriptors[2];
    assert!(lore.description.contains("Domain-specific Crew-owned"));
    assert!(lore.description.contains("governance and provenance"));
    assert_eq!(lore.module_id.as_deref(), Some("roleplay_lore"));
}
