use rusty_crew_core_bridge_api::{
    BrainImplementationHandle, BrainImplementationRegistration, CoreError, CoreErrorKind,
    CoreEvent, CoreResult, PlatformAdapterHandle, PlatformAdapterRegistration, SubscriptionHandle,
};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;

#[derive(Debug)]
pub(crate) struct BrainImplementationRegistry {
    next_handle: u64,
    by_handle: HashMap<BrainImplementationHandle, BrainImplementationRegistration>,
    by_implementation_id:
        HashMap<rusty_crew_core_bridge_api::BrainImplementationId, BrainImplementationHandle>,
    by_profile_id: HashMap<rusty_crew_core_bridge_api::ProfileId, BrainImplementationHandle>,
}

impl BrainImplementationRegistry {
    pub(crate) fn new() -> Self {
        Self {
            next_handle: 1,
            by_handle: HashMap::new(),
            by_implementation_id: HashMap::new(),
            by_profile_id: HashMap::new(),
        }
    }

    pub(crate) fn register(
        &mut self,
        registration: BrainImplementationRegistration,
    ) -> CoreResult<BrainImplementationHandle> {
        validate_brain_registration(&registration)?;

        if self
            .by_implementation_id
            .contains_key(&registration.implementation_id)
        {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "brain implementation {} is already registered",
                    registration.implementation_id
                ),
            ));
        }

        if self.by_profile_id.contains_key(&registration.profile_id) {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "brain implementation for profile {} is already registered",
                    registration.profile_id
                ),
            ));
        }

        let handle = BrainImplementationHandle::new(self.next_handle);
        self.next_handle = self.next_handle.checked_add(1).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "brain implementation handle overflow",
            )
        })?;

        self.by_implementation_id
            .insert(registration.implementation_id.clone(), handle);
        self.by_profile_id
            .insert(registration.profile_id.clone(), handle);
        self.by_handle.insert(handle, registration);

        Ok(handle)
    }

    pub(crate) fn replace_for_profile(
        &mut self,
        registration: BrainImplementationRegistration,
    ) -> CoreResult<BrainImplementationHandle> {
        validate_brain_registration(&registration)?;

        let Some(handle) = self.by_profile_id.get(&registration.profile_id).copied() else {
            return self.register(registration);
        };

        if let Some(existing_handle) = self
            .by_implementation_id
            .get(&registration.implementation_id)
            .copied()
        {
            if existing_handle != handle {
                return Err(CoreError::new(
                    CoreErrorKind::AlreadyExists,
                    format!(
                        "brain implementation {} is already registered",
                        registration.implementation_id
                    ),
                ));
            }
        }

        let previous = self.by_handle.insert(handle, registration.clone());
        if let Some(previous) = previous {
            self.by_implementation_id
                .remove(&previous.implementation_id);
        }
        self.by_implementation_id
            .insert(registration.implementation_id.clone(), handle);
        self.by_profile_id
            .insert(registration.profile_id.clone(), handle);

        Ok(handle)
    }

    pub(crate) fn get(
        &self,
        handle: BrainImplementationHandle,
    ) -> CoreResult<&BrainImplementationRegistration> {
        self.by_handle.get(&handle).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::BrainUnavailable,
                format!(
                    "brain implementation handle {} is not registered",
                    handle.get()
                ),
            )
        })
    }

    pub(crate) fn unregister_for_profile(
        &mut self,
        profile_id: &rusty_crew_core_bridge_api::ProfileId,
    ) -> CoreResult<BrainImplementationHandle> {
        let handle = self.by_profile_id.remove(profile_id).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("brain implementation for profile {profile_id} is not registered"),
            )
        })?;
        let registration = self.by_handle.remove(&handle).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::BrainUnavailable,
                format!(
                    "brain implementation handle {} is not registered",
                    handle.get()
                ),
            )
        })?;
        self.by_implementation_id
            .remove(&registration.implementation_id);
        Ok(handle)
    }

    pub(crate) fn registrations(&self) -> impl Iterator<Item = &BrainImplementationRegistration> {
        self.by_handle.values()
    }
}

#[derive(Debug)]
pub(crate) struct SubscriptionRecord {
    pub(crate) bus_subscription_id: u64,
    receiver: Receiver<CoreEvent>,
}

#[derive(Debug)]
pub(crate) struct SubscriptionRegistry {
    next_handle: u64,
    by_handle: HashMap<SubscriptionHandle, SubscriptionRecord>,
}

impl SubscriptionRegistry {
    pub(crate) fn new() -> Self {
        Self {
            next_handle: 1,
            by_handle: HashMap::new(),
        }
    }

    pub(crate) fn insert(
        &mut self,
        bus_subscription_id: u64,
        receiver: Receiver<CoreEvent>,
    ) -> SubscriptionHandle {
        let handle = SubscriptionHandle::new(self.next_handle);
        self.next_handle += 1;
        self.by_handle.insert(
            handle,
            SubscriptionRecord {
                bus_subscription_id,
                receiver,
            },
        );
        handle
    }

    pub(crate) fn remove(&mut self, handle: SubscriptionHandle) -> CoreResult<SubscriptionRecord> {
        self.by_handle.remove(&handle).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("subscription handle {} is not registered", handle.get()),
            )
        })
    }

    pub(crate) fn clear(&mut self) {
        self.by_handle.clear();
    }

    pub(crate) fn drain(
        &self,
        handle: SubscriptionHandle,
        max_events: u32,
    ) -> CoreResult<Vec<CoreEvent>> {
        let record = self.by_handle.get(&handle).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::NotFound,
                format!("subscription handle {} is not registered", handle.get()),
            )
        })?;
        let mut events = Vec::new();
        for _ in 0..max_events {
            match record.receiver.try_recv() {
                Ok(event) => events.push(event),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            }
        }
        Ok(events)
    }
}

#[derive(Debug)]
pub(crate) struct PlatformAdapterRegistry {
    next_handle: u64,
    by_handle: HashMap<PlatformAdapterHandle, PlatformAdapterRegistration>,
    by_adapter_id: HashMap<rusty_crew_core_bridge_api::AdapterId, PlatformAdapterHandle>,
}

impl PlatformAdapterRegistry {
    pub(crate) fn new() -> Self {
        Self {
            next_handle: 1,
            by_handle: HashMap::new(),
            by_adapter_id: HashMap::new(),
        }
    }

    pub(crate) fn register(
        &mut self,
        registration: PlatformAdapterRegistration,
    ) -> CoreResult<PlatformAdapterHandle> {
        if registration.adapter_id.0.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "platform adapter requires an adapter_id",
            ));
        }
        if registration.display_name.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "platform adapter requires a display_name",
            ));
        }
        if self.by_adapter_id.contains_key(&registration.adapter_id) {
            return Err(CoreError::new(
                CoreErrorKind::AlreadyExists,
                format!(
                    "platform adapter {} is already registered",
                    registration.adapter_id
                ),
            ));
        }

        let handle = PlatformAdapterHandle::new(self.next_handle);
        self.next_handle = self.next_handle.checked_add(1).ok_or_else(|| {
            CoreError::new(
                CoreErrorKind::InvalidInput,
                "platform adapter handle overflow",
            )
        })?;
        self.by_adapter_id
            .insert(registration.adapter_id.clone(), handle);
        self.by_handle.insert(handle, registration);
        Ok(handle)
    }
}

fn validate_brain_registration(registration: &BrainImplementationRegistration) -> CoreResult<()> {
    if registration.implementation_id.0.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain implementation requires an implementation_id",
        ));
    }
    if registration.profile_id.0.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain implementation requires a profile_id",
        ));
    }
    if registration.model_config.provider.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain implementation requires a model provider",
        ));
    }
    if registration.model_config.model_name.trim().is_empty() {
        return Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            "brain implementation requires a model name",
        ));
    }
    if let Some(strategy) = &registration.strategy {
        if strategy.module_id.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "brain strategy metadata requires a module_id",
            ));
        }
        if strategy.strategy_id.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "brain strategy metadata requires a strategy_id",
            ));
        }
    }
    let mut tool_names = HashSet::new();
    for tool in &registration.tool_profile.tools {
        if tool.name.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                "brain implementation tool name must be non-empty",
            ));
        }
        if tool.description.trim().is_empty() {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!(
                    "brain implementation tool {} requires a description",
                    tool.name
                ),
            ));
        }
        if !tool_names.insert(tool.name.clone()) {
            return Err(CoreError::new(
                CoreErrorKind::InvalidInput,
                format!("brain implementation has duplicate tool {}", tool.name),
            ));
        }
    }
    Ok(())
}
