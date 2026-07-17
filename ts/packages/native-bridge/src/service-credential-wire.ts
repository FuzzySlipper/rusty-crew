import type {
  NativeModelProviderCredentialKind,
  NativeModelProviderCredentialLink,
  NativeModelProviderCredentialLinkResult,
  NativeModelProviderCredentialUnlink,
  NativeServiceCredentialQuery,
  NativeServiceCredentialRecord,
  NativeServiceCredentialWrite,
} from "./public-api.js";
import {
  toNativeModelProviderRecord,
  type RawModelProviderCredential,
  type RawModelProviderRecord,
} from "./profile-provider-wire.js";

export interface RawServiceCredentialRecord {
  credential_id: string;
  display_name: string;
  provider_kind: string;
  credential_kind: NativeModelProviderCredentialKind;
  credential: RawModelProviderCredential;
  linked_provider_aliases: string[];
  revision: number;
  created_at: string;
  updated_at: string;
}

export interface RawServiceCredentialWrite {
  credential_id: string;
  display_name: string;
  provider_kind: string;
  credential_kind: NativeModelProviderCredentialKind;
  secret?: string;
  clear_secret: boolean;
  expected_revision?: number;
  now: string;
}

export interface RawServiceCredentialQuery {
  provider_kind?: string;
  limit?: number;
  offset?: number;
}

export interface RawModelProviderCredentialLink {
  provider_alias: string;
  credential_id: string;
  expected_provider_revision?: number;
  expected_credential_revision?: number;
  now: string;
}

export interface RawModelProviderCredentialUnlink {
  provider_alias: string;
  expected_provider_revision?: number;
  now: string;
}

export interface RawModelProviderCredentialLinkResult {
  provider: RawModelProviderRecord;
  credential: RawServiceCredentialRecord;
}

export const serviceCredentialWire = {
  toNativeRecord(
    record: RawServiceCredentialRecord,
  ): NativeServiceCredentialRecord {
    return {
      credentialId: record.credential_id,
      displayName: record.display_name,
      providerKind: record.provider_kind,
      credentialKind: record.credential_kind,
      credential: {
        hasSecret: record.credential.has_secret,
        secretRef: record.credential.secret_ref ?? undefined,
        updatedAt: record.credential.updated_at ?? undefined,
        kind: record.credential.kind ?? undefined,
        revision: record.credential.revision ?? undefined,
      },
      linkedProviderAliases: record.linked_provider_aliases,
      revision: record.revision,
      createdAt: record.created_at,
      updatedAt: record.updated_at,
    };
  },

  toRawWrite(write: NativeServiceCredentialWrite): RawServiceCredentialWrite {
    return {
      credential_id: write.credentialId,
      display_name: write.displayName,
      provider_kind: write.providerKind,
      credential_kind: write.credentialKind,
      secret: write.secret,
      clear_secret: write.clearSecret ?? false,
      expected_revision: write.expectedRevision,
      now: write.now,
    };
  },

  toRawQuery(query: NativeServiceCredentialQuery): RawServiceCredentialQuery {
    return {
      provider_kind: query.providerKind,
      limit: query.limit,
      offset: query.offset,
    };
  },

  toRawLink(
    link: NativeModelProviderCredentialLink,
  ): RawModelProviderCredentialLink {
    return {
      provider_alias: link.providerAlias,
      credential_id: link.credentialId,
      expected_provider_revision: link.expectedProviderRevision,
      expected_credential_revision: link.expectedCredentialRevision,
      now: link.now,
    };
  },

  toRawUnlink(
    unlink: NativeModelProviderCredentialUnlink,
  ): RawModelProviderCredentialUnlink {
    return {
      provider_alias: unlink.providerAlias,
      expected_provider_revision: unlink.expectedProviderRevision,
      now: unlink.now,
    };
  },

  toNativeLinkResult(
    result: RawModelProviderCredentialLinkResult,
  ): NativeModelProviderCredentialLinkResult {
    return {
      provider: toNativeModelProviderRecord(result.provider),
      credential: serviceCredentialWire.toNativeRecord(result.credential),
    };
  },
};
