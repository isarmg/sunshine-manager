import { InstanceNameField } from "../shell/index.js";
import {useState,type FormEvent} from "react";
import {Button,Dialog,ErrorState,FormField,TextField} from "@sarmg/admin-ui";
import {useAdminApplication,errorRequestId} from "../shell/index.js";
import {CURRENT_API_PREFIX,isTicket,type Ticket} from "./api";
export function TicketPanel({ticket}:{ticket:Ticket}){
 return <section className="sarmg-content-panel"><h2>实例配对码</h2><p>配对码仅本次显示，不设有效期；配对成功或手动取消后失效。请在 Sunshine 主机的受保护 bootstrap.json 中填写；不要放入命令行、日志或聊天。</p>
 <dl><dt>Manager ID</dt><dd>{ticket.manager_id}</dd><dt>设备 ID</dt><dd>{ticket.device.id}</dd><dt>配对码</dt><dd><code className="sunshine-token">{ticket.token}</code></dd></dl>
 <p>配置 Manager 的 WSS 地址和可信 CA、本机 Sunshine 的 HTTPS 地址及凭据后，按安装手册运行 sunshine-agent init，再启动 Agent 服务。Manager 不接收 Sunshine 密码。</p></section>;
}
export function DeviceRegistrationDialog({close,created}:{close():void;created(ticket:Ticket):void}){
 const{client}=useAdminApplication();const[pending,setPending]=useState(false);const[failure,setFailure]=useState<{requestId?:string}|null>(null);
 async function submit(event:FormEvent<HTMLFormElement>){event.preventDefault();if(pending)return;const name=String(new FormData(event.currentTarget).get("name")??"").trim();setPending(true);setFailure(null);
 try{created(await client.request(`${CURRENT_API_PREFIX}/sunshine/devices`,isTicket,{method:"POST",body:JSON.stringify({name})}));}catch(error){setFailure({requestId:errorRequestId(error)})}finally{setPending(false)}}
 return <Dialog title="新建 Sunshine 实例" description="注册安装在 Sunshine 主机上的独立 Agent；不会安装 Sunshine 或改变串流链路。" onClose={()=>{if(!pending)close()}}>
 <form onSubmit={event=>void submit(event)}>{failure&&<ErrorState requestId={failure.requestId}>创建未能确认，请先刷新实例列表核对。</ErrorState>}
 <FormField label="实例名称"><InstanceNameField name="name" required title="最多 32 个字符" readOnly={pending} data-sarmg-initial-focus/></FormField><p>最多 32 个字符。</p>
 <div className="sarmg-actions"><Button disabled={pending} onClick={close}>取消</Button><Button type="submit" disabled={pending}>创建实例</Button></div></form></Dialog>;
}
