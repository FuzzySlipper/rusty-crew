use super::*;

impl CoreEngine {
    pub fn apply_curator_governance_write(
        &self,
        write: &CuratorGovernanceWrite,
    ) -> CoreResult<CuratorGovernanceWriteResult> {
        self.store.apply_curator_governance_write(write)
    }

    pub fn get_curator_candidate(
        &self,
        candidate_id: &str,
    ) -> CoreResult<Option<CuratorCandidateRecord>> {
        self.store.get_curator_candidate(candidate_id)
    }

    pub fn list_curator_candidates(
        &self,
        query: &CuratorCandidateQuery,
    ) -> CoreResult<ExactPage<CuratorCandidateRecord>> {
        self.store.list_curator_candidates(query)
    }

    pub fn get_curator_mutation(
        &self,
        mutation_id: &str,
    ) -> CoreResult<Option<CuratorMutationRecord>> {
        self.store.get_curator_mutation(mutation_id)
    }

    pub fn list_curator_mutations(
        &self,
        query: &CuratorMutationQuery,
    ) -> CoreResult<ExactPage<CuratorMutationRecord>> {
        self.store.list_curator_mutations(query)
    }

    pub fn list_curator_audit_receipts(
        &self,
        query: &CuratorAuditQuery,
    ) -> CoreResult<ExactPage<CuratorAuditReceiptRecord>> {
        self.store.list_curator_audit_receipts(query)
    }
}
