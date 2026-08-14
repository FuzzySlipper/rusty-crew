import type {
  InstallDiplomatBindingQuery,
  InstallDiplomatBindingRecord,
  InstallDiplomatBindingStatusUpdate,
  InstallDiplomatBindingWrite,
  InstallDiplomatRebindRequest,
  TelegramDiplomatIngressPlan,
  TelegramDiplomatIngressRequest,
  TelegramOperatorConsultQuery,
  TelegramOperatorConsultRecord,
  TelegramOperatorConsultRequest,
  TelegramOperatorConsultSettlement,
} from "@rusty-crew/contracts";

export type NativeInstallDiplomatBindingQuery = InstallDiplomatBindingQuery;
export type NativeInstallDiplomatBindingRecord = InstallDiplomatBindingRecord;
export type NativeInstallDiplomatBindingStatusUpdate =
  InstallDiplomatBindingStatusUpdate;
export type NativeInstallDiplomatBindingWrite = InstallDiplomatBindingWrite;
export type NativeInstallDiplomatRebindRequest = InstallDiplomatRebindRequest;
export type NativeTelegramDiplomatIngressPlan = TelegramDiplomatIngressPlan;
export type NativeTelegramDiplomatIngressRequest =
  TelegramDiplomatIngressRequest;
export type NativeTelegramOperatorConsultQuery = TelegramOperatorConsultQuery;
export type NativeTelegramOperatorConsultRecord = TelegramOperatorConsultRecord;
export type NativeTelegramOperatorConsultRequest =
  TelegramOperatorConsultRequest;
export type NativeTelegramOperatorConsultSettlement =
  TelegramOperatorConsultSettlement;

export interface NativeInstallDiplomatBridgeMethods {
  putInstallDiplomatBinding(
    write: NativeInstallDiplomatBindingWrite,
  ): Promise<NativeInstallDiplomatBindingRecord>;
  rebindInstallDiplomat(
    request: NativeInstallDiplomatRebindRequest,
  ): Promise<NativeInstallDiplomatBindingRecord>;
  setInstallDiplomatBindingStatus(
    update: NativeInstallDiplomatBindingStatusUpdate,
  ): Promise<NativeInstallDiplomatBindingRecord>;
  getInstallDiplomatBinding(
    bindingId: string,
  ): Promise<NativeInstallDiplomatBindingRecord | undefined>;
  listInstallDiplomatBindings(
    query?: NativeInstallDiplomatBindingQuery,
  ): Promise<NativeInstallDiplomatBindingRecord[]>;
  planTelegramDiplomatIngress(
    request: NativeTelegramDiplomatIngressRequest,
  ): Promise<NativeTelegramDiplomatIngressPlan>;
  requestTelegramOperatorConsult(
    request: NativeTelegramOperatorConsultRequest,
  ): Promise<NativeTelegramOperatorConsultRecord>;
  prepareTelegramOperatorConsultDelivery(
    consultId: string,
  ): Promise<NativeTelegramOperatorConsultRecord>;
  settleTelegramOperatorConsult(
    settlement: NativeTelegramOperatorConsultSettlement,
  ): Promise<NativeTelegramOperatorConsultRecord>;
  listTelegramOperatorConsults(
    query?: NativeTelegramOperatorConsultQuery,
  ): Promise<NativeTelegramOperatorConsultRecord[]>;
}
