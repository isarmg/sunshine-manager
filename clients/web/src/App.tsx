import {createSarmgAdminApplication,errorRequestId,useAdminApplication} from "../shell/index.js";
import {Button,EmptyState,ErrorState,LoadingState} from "@sarmg/admin-ui";
import {useEffect,useState} from "react";
import {CURRENT_API_PREFIX,adminApi,isDevices,type DeviceInfo,type Ticket} from "./api";
import {DeviceRegistrationDialog} from "./DeviceRegistrationDialog";
import {AgentWorkspace} from "./AgentWorkspace";
import {DeviceInstances} from "./DeviceInstances";
import { HeaderNavigation, InstanceHeaderActions } from "../shell/index.js";
const pages = [["instances","实例"],["status","设备状态"],["config","Sunshine 配置"],["tasks","任务记录"]] as const;
function currentPage(){return pages.find(([id])=>id===window.location.hash.slice(1))?.[0]??"instances"}
function DevicesPage(){
 const{client}=useAdminApplication();const[devices,setDevices]=useState<DeviceInfo[]|null>(null);const[failure,setFailure]=useState<{requestId?:string}|null>(null);
 const[generation,setGeneration]=useState(0);const[creating,setCreating]=useState(false);const[selected,setSelected]=useState<string|null>(null);const[ticket,setTicket]=useState<Ticket|null>(null);
 const[page,setPage]=useState(currentPage);
 useEffect(()=>{const changed=()=>setPage(currentPage());window.addEventListener("hashchange",changed);return()=>window.removeEventListener("hashchange",changed)},[]);
 useEffect(()=>{const controller=new AbortController();let active=true;async function load(){try{const values=await client.request(CURRENT_API_PREFIX+"/sunshine/devices",isDevices,{signal:controller.signal});if(active){setDevices(values);setFailure(null)}}catch(error){if(active)setFailure({requestId:errorRequestId(error)})}}void load();const timer=setInterval(()=>void load(),5000);return()=>{active=false;controller.abort();clearInterval(timer)}},[client,generation]);
 const refresh=()=>setGeneration(value=>value+1);const device=devices?.find(value=>value.id===selected)??devices?.[0];
 return <section><InstanceHeaderActions create={()=>setCreating(true)} refresh={refresh}/><HeaderNavigation label="Sunshine 页面">{pages.map(([id,name])=><Button key={id} aria-pressed={page===id} onClick={()=>{window.location.hash=id}}>{name}</Button>)}</HeaderNavigation><h1 className="sarmg-visually-hidden">Sunshine 设备管理</h1>
 {failure&&<ErrorState requestId={failure.requestId} onRetry={refresh}>无法加载设备列表</ErrorState>}
 {page==="instances"?<section aria-label="Sunshine 实例"><h2>实例</h2>{devices===null?<LoadingState>正在加载设备…</LoadingState>:<DeviceInstances devices={devices} select={id=>{setSelected(id);window.location.hash="status"}}/>}</section>
 :device?<><h2>{device.name}</h2><AgentWorkspace key={device.id} device={device} page={pages.find(([id])=>id===page)![1]} changed={refresh} ticket={ticket?.device.id===device.id?ticket:null}/></>:devices===null?<LoadingState>正在加载设备…</LoadingState>:<EmptyState>暂无实例，请新建并注册 Agent。</EmptyState>}
 {creating&&<DeviceRegistrationDialog close={()=>setCreating(false)} created={value=>{setTicket(value);setSelected(value.device.id);setCreating(false);window.location.hash="status";refresh()}}/>}</section>
}
export default createSarmgAdminApplication({product:{name:"Sunshine Manager"},client:adminApi,navigation:[],routes:<DevicesPage/>});
