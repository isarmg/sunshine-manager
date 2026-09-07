import { t } from "../shell/i18n.js";
import { InstanceNameField } from "../shell/index.js";
import {useState,type FormEvent} from "react";
import {Button,Dialog,ErrorState,FormField,TextField} from "@sarmg/admin-ui";
import {useAdminApplication,errorRequestId} from "../shell/index.js";
import {CURRENT_API_PREFIX,isTicket,type Ticket} from "./api";
export function TicketPanel({ticket}:{ticket:Ticket}){
 return <section className="sarmg-content-panel"><h2>{t("实例配对码", "Instance pairing code")}</h2><p>{t("配对码仅本次显示，不设有效期；配对成功或手动取消后失效。请在 Sunshine 主机的受保护 bootstrap.json 中填写；不要放入命令行、日志或聊天。", "This code is shown only once and has no expiry. Pairing or manual cancellation invalidates it. Enter it in the protected bootstrap.json on the Sunshine host; never place it in command lines, logs or chats.")}</p>
 <dl><dt>{t("管理端标识", "Manager ID")}</dt><dd>{ticket.manager_id}</dd><dt>{t("设备标识", "Device ID")}</dt><dd>{ticket.device.id}</dd><dt>{t("配对码", "Pairing code")}</dt><dd><code className="sunshine-token">{ticket.token}</code></dd></dl>
 <p>{t("配置 管理端 的 WSS 地址和可信 CA、本机 Sunshine 的 HTTPS 地址及凭据后，按安装手册运行 sunshine-client init，再启动 客户端 服务。管理端 不接收 Sunshine 密码。", "Configure the manager WSS address and trusted CA, and the local Sunshine HTTPS address and credentials. Follow the installation guide to run sunshine-client init, then start the client service. The manager never receives the Sunshine password.")}</p></section>;
}
export function DeviceRegistrationDialog({close,created}:{close():void;created(ticket:Ticket):void}){
 const{client}=useAdminApplication();const[pending,setPending]=useState(false);const[failure,setFailure]=useState<{requestId?:string}|null>(null);
 async function submit(event:FormEvent<HTMLFormElement>){event.preventDefault();if(pending)return;const name=String(new FormData(event.currentTarget).get("name")??"").trim();setPending(true);setFailure(null);
 try{created(await client.request(`${CURRENT_API_PREFIX}/sunshine/devices`,isTicket,{method:"POST",body:JSON.stringify({name})}));}catch(error){setFailure({requestId:errorRequestId(error)})}finally{setPending(false)}}
 return <Dialog title={t("新建 Sunshine 实例", "Create Sunshine instance")} description={t("注册安装在 Sunshine 主机上的独立 客户端；不会安装 Sunshine 或改变串流链路。", "Register an independent client on the Sunshine host. This does not install Sunshine or change the streaming connection.")} onClose={()=>{if(!pending)close()}}>
 <form onSubmit={event=>void submit(event)} aria-busy={pending}>{failure&&<ErrorState requestId={failure.requestId}>{t("创建未能确认，请先刷新实例列表核对。", "Creation could not be confirmed. Refresh the instance list and check first.")}</ErrorState>}
 <FormField label={t("实例名称", "Instance name")}><InstanceNameField name="name" required title={t("最多 32 个字符", "Up to 32 characters")} readOnly={pending} data-sarmg-initial-focus/></FormField><p>{t("最多 32 个字符。", "Up to 32 characters.")}</p>
 <div className="sarmg-actions"><Button disabled={pending} onClick={close}>{t("取消", "Cancel")}</Button><Button type="submit" disabled={pending}>{t("创建实例", "Create instance")}</Button></div></form></Dialog>;
}
