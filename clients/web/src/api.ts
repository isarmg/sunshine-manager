import { createAdministratorApiClient } from "@sarmg/admin-web";
import { isErrorEnvelope, type ErrorEnvelope } from "@sarmg/contracts";
import { isApiClientError } from "@sarmg/http-client";
export const CURRENT_API_PREFIX = "/api/v2";
export const adminApi = createAdministratorApiClient();
export type Snapshot = {revision:string;sunshine_version:string;fields:Record<string,string>;effectiveness:"pending_verification"|"awaiting_restart"};
export type DeviceInfo = {id:string;name:string;registered:boolean;pairing_pending:boolean;revoked:boolean;agent_online:boolean;sunshine_reachable:boolean|null;configuration_state:string;snapshot:Snapshot|null;capabilities:{protocol:string;agent_version:string;os:string;sunshine_version:string;restart_allowed:boolean;managed_fields:string[]}|null;last_seen_at_micros:number|null};
export type Ticket={device:DeviceInfo;manager_id:string;token:string};
export type Command={kind:"read_config"}|{kind:"patch_config";expected_revision:string;set:Record<string,string|number|boolean>;remove:string[];restart_policy:"manual"}|{kind:"restart";expected_revision:string;administrator_confirmed:true};
export type Report={kind:string;snapshot?:Snapshot;reason?:string;actual_revision?:string};
export type Operation={operation_id:string;device_id:string;action:string;state:"pending"|"running"|"succeeded"|"failed"|"unknown"|"dead_letter"|"resolved";attempt:number;created_at_micros:number;updated_at_micros:number;result:Report|null;reconciliation:Report|null;resolution:string|null};
export function record(value:unknown):value is Record<string,unknown>{return typeof value==="object"&&value!==null&&!Array.isArray(value)}
export function isSnapshot(value:unknown):value is Snapshot{return record(value)&&typeof value.revision==="string"&&/^[a-f0-9]{64}$/.test(value.revision)&&typeof value.sunshine_version==="string"&&record(value.fields)&&Object.values(value.fields).every(field=>typeof field==="string")&&(value.effectiveness==="pending_verification"||value.effectiveness==="awaiting_restart")}
export function isDevice(value:unknown):value is DeviceInfo{
 return record(value)&&typeof value.id==="string"&&typeof value.name==="string"&&typeof value.registered==="boolean"&&typeof value.pairing_pending==="boolean"&&typeof value.revoked==="boolean"&&typeof value.agent_online==="boolean"&&(value.sunshine_reachable===null||typeof value.sunshine_reachable==="boolean")&&typeof value.configuration_state==="string"&&(value.snapshot===null||isSnapshot(value.snapshot))&&(value.capabilities===null||(record(value.capabilities)&&typeof value.capabilities.restart_allowed==="boolean"&&Array.isArray(value.capabilities.managed_fields)))&&(value.last_seen_at_micros===null||typeof value.last_seen_at_micros==="number");
}
export function isDevices(value:unknown):value is DeviceInfo[]{return Array.isArray(value)&&value.every(isDevice)}
export function isTicket(value:unknown):value is Ticket{return record(value)&&isDevice(value.device)&&typeof value.manager_id==="string"&&typeof value.token==="string"}
function isReport(value:unknown):value is Report{return record(value)&&typeof value.kind==="string"&&(!("snapshot" in value)||isSnapshot(value.snapshot))}
export function isOperation(value:unknown):value is Operation{return record(value)&&typeof value.operation_id==="string"&&typeof value.device_id==="string"&&typeof value.action==="string"&&typeof value.state==="string"&&["pending","running","succeeded","failed","unknown","dead_letter","resolved"].includes(value.state)&&(value.result===null||isReport(value.result))&&(value.reconciliation===null||isReport(value.reconciliation))}
export function isOperations(value:unknown):value is Operation[]{return Array.isArray(value)&&value.every(isOperation)}
export function currentErrorEnvelope(error:unknown):ErrorEnvelope|undefined{if(!isApiClientError(error))return undefined;return isErrorEnvelope(error.envelope)?error.envelope:undefined}
