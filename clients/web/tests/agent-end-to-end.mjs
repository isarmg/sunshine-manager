import assert from "node:assert/strict";
import {execFile,spawn} from "node:child_process";
import {promisify} from "node:util";
import {mkdtemp,readFile,writeFile,open,rm} from "node:fs/promises";
import {join,resolve} from "node:path";
import {tmpdir} from "node:os";
import {createServer as httpsServer} from "node:https";
import {request as httpRequest} from "node:http";
import {connect} from "node:net";
import {randomBytes} from "node:crypto";
import {DatabaseSync} from "node:sqlite";
import {chromium,expect} from "@playwright/test";
import {withLocalServer} from "./local-server.mjs";

// Actual browser, Manager, TLS ingress and independent Agent process. Only Sunshine is a fixture.
const root=await mkdtemp(join(tmpdir(),"sunshine-agent-e2e-"));
const exec=promisify(execFile);
let sunshine,ingress,agent,agentLog;
const tunnels=new Set();
const agentBinary=resolve("../../target/debug/sunshine-agent");
async function stopAgent(){
 if(agent&&agent.exitCode===null&&agent.signalCode===null){
  const stopped=new Promise((done,fail)=>{const timer=setTimeout(()=>fail(new Error("Test Agent failed to stop; state retained at "+root)),10000);agent.once("exit",()=>{clearTimeout(timer);done()})});
  agent.kill("SIGTERM");await stopped;
 }
}
async function startAgent(state){
 agent=spawn(agentBinary,["run","--state",state],{stdio:["ignore",agentLog.fd,agentLog.fd]});
 await new Promise((done,fail)=>{agent.once("spawn",done);agent.once("error",fail)});
}
try {
 const caKey=join(root,"ca.key"),caCert=join(root,"ca.crt"),key=join(root,"server.key"),csr=join(root,"server.csr"),cert=join(root,"server.crt");
 await exec("openssl",["req","-x509","-newkey","rsa:2048","-nodes","-keyout",caKey,"-out",caCert,"-days","1","-subj","/CN=Agent test CA","-addext","basicConstraints=critical,CA:TRUE"]);
 await exec("openssl",["req","-new","-newkey","rsa:2048","-nodes","-keyout",key,"-out",csr,"-subj","/CN=localhost","-addext","subjectAltName=DNS:localhost,IP:127.0.0.1","-addext","basicConstraints=critical,CA:FALSE","-addext","extendedKeyUsage=serverAuth"]);
 await exec("openssl",["x509","-req","-in",csr,"-CA",caCert,"-CAkey",caKey,"-CAcreateserial","-out",cert,"-days","1","-copy_extensions","copy"]);
 const tls={key:await readFile(key),cert:await readFile(cert)};
 let config={sunshine_name:"Original",qp:"28",output_name:"DISPLAY1",custom_setting:"preserved","global_prep_cmd":"LOCAL_ONLY_SECRET_COMMAND"};
 let writes=0,restarts=0;let loseRestartReceipt=false;
 sunshine=httpsServer(tls,async(req,res)=>{
  const json=value=>{res.setHeader("content-type","application/json");res.end(JSON.stringify(value))};
  if(req.headers.authorization!=="Basic "+Buffer.from("fixture:local-only-password").toString("base64")){res.statusCode=401;return json({status:false});}
  if(req.method==="GET"&&req.url==="/api/config")return json({status:true,platform:"linux",version:"2026.516.143833",...config});
  if(req.method==="POST"&&req.url==="/api/config"){
   let body="";for await(const chunk of req)body+=chunk;
   config=JSON.parse(body);writes++;return json({status:true});
  }
  if(req.method==="POST"&&req.url==="/api/restart"){restarts++;if(loseRestartReceipt)return res.destroy();return json({status:true});}
  res.statusCode=404;json({status:false});
 });
 await new Promise(done=>sunshine.listen(0,"127.0.0.1",done));
 await withLocalServer({prefix:"SUNSHINE_MANAGER",binary:"../../target/debug/sunshine-manager",extraEnv:{SUNSHINE_MANAGER_PRODUCTION:"false",SUNSHINE_MANAGER_CREDENTIAL_KEY:randomBytes(32).toString("base64"),SUNSHINE_MANAGER_CREDENTIAL_KEY_ID:"test"}},async({base,password,database})=>{
  const target=new URL(base);
  ingress=httpsServer(tls,(req,res)=>{
   const upstream=httpRequest(base+req.url,{method:req.method,headers:{...req.headers,"x-forwarded-proto":"https"}},reply=>{res.writeHead(reply.statusCode,reply.headers);reply.pipe(res)});
   upstream.on("error",()=>{res.statusCode=502;res.end()});req.pipe(upstream);
  });
  ingress.on("upgrade",(req,socket,head)=>{
   const upstream=connect(Number(target.port),"127.0.0.1",()=>{
    let headers=req.method+" "+req.url+" HTTP/1.1\r\n";
    for(const [name,value] of Object.entries({...req.headers,"x-forwarded-proto":"https"}))headers+=name+": "+value+"\r\n";
    upstream.write(headers+"\r\n");if(head.length)upstream.write(head);socket.pipe(upstream).pipe(socket);
   });
   tunnels.add(socket);socket.on("close",()=>{tunnels.delete(socket);upstream.destroy()});
   socket.on("error",()=>upstream.destroy());upstream.on("error",()=>socket.destroy());
  });
  await new Promise(done=>ingress.listen(0,"127.0.0.1",done));
  const browser=await chromium.launch();const errors=[];
  try {
   const page=await browser.newPage({ locale: "zh-CN" });page.on("pageerror",e=>errors.push(e.message));
   await page.goto(base);await page.getByLabel("用户名",{exact:true}).fill("admin");await page.getByLabel("密码",{exact:true}).fill(password);
   await page.getByRole("button",{name:"登录",exact:true}).click();
   await page.getByRole("button",{name:"新建实例",exact:true}).click();
   await page.getByLabel("实例名称",{exact:true}).fill("Agent 闭环测试");
   const created=page.waitForResponse(r=>r.url().endsWith("/sunshine/devices")&&r.request().method()==="POST").then(async response=>{await response.finished();return response.json()});
   await page.getByRole("button",{name:"创建实例",exact:true}).click();
   const ticket=await created;const id=ticket.device.id;const api="/api/v2/sunshine/devices/"+id;
   const bootstrap=join(root,"bootstrap.json"),state=join(root,"agent");
   await writeFile(bootstrap,JSON.stringify({manager_endpoint:"wss://127.0.0.1:"+ingress.address().port+"/sunshine-agent/v1/connect",manager_ca_pem:await readFile(caCert,"utf8"),manager_id:ticket.manager_id,device_id:id,enrollment_token:ticket.token,sunshine_endpoint:"https://127.0.0.1:"+sunshine.address().port+"/",sunshine_ca_pem:await readFile(caCert,"utf8"),sunshine_username:"fixture",sunshine_password:"local-only-password",restart_allowed:true}),{mode:0o600});
   await exec(agentBinary,["init","--state",state,"--bootstrap",bootstrap]);
   agentLog=await open(join(root,"agent.log"),"wx",0o600);await startAgent(state);
   const device=async()=>{const r=await page.request.get(base+"/api/v2/sunshine/devices");return(await r.json()).find(d=>d.id===id)};
   await expect.poll(async()=>(await device()).agent_online,{timeout:20000}).toBe(true);
   await expect.poll(async()=>(await device()).sunshine_reachable,{timeout:20000}).toBe(true);
   await page.getByRole("button",{name:"刷新",exact:true}).click();
   await page.getByRole("button",{name:"Sunshine 配置",exact:true}).click();
   await page.getByLabel("Sunshine 名称", { exact: true }).fill("网页保存");
   await page.getByRole("button",{name:"预览变更",exact:true}).click();
   const queued=page.waitForResponse(r=>r.url().endsWith("/tasks")&&r.request().method()==="POST");
   await page.getByRole("button",{name:"确认保存配置",exact:true}).click();
   const operation=await(await queued).json();
   const getOperation=async id=>(await(await page.request.get(base+"/api/v2/sunshine/operations/"+id)).json());
   await expect.poll(async()=>(await getOperation(operation.operation_id)).state,{timeout:20000}).toBe("succeeded");
   assert.equal(writes,1);assert.equal(restarts,0);assert.equal(config.sunshine_name,"网页保存");assert.equal(config.output_name,"DISPLAY1");assert.equal(config.custom_setting,"preserved");assert.equal(config.global_prep_cmd,"LOCAL_ONLY_SECRET_COMMAND");assert.ok(!("status" in config));
   const saved=await device();assert.equal(saved.configuration_state,"awaiting_restart");
   assert.ok(!JSON.stringify(saved).includes("LOCAL_ONLY_SECRET_COMMAND"));
   // Offline submission is durable, and a fresh Agent process reuses its registered identity.
   await stopAgent();
   const csrf=(await queued).request().headers()["x-csrf-token"];
   const submit=async command=>{const r=await page.request.post(base+api+"/tasks",{headers:{"Origin":base,"Sec-Fetch-Site":"same-origin","x-csrf-token":csrf,"Idempotency-Key":randomBytes(16).toString("hex")},data:command});assert.equal(r.status(),202,"Task admission: "+(r.ok()?"ok":(await r.json()).code));return r.json()};
   const offline=await submit({kind:"read_config"});assert.equal(offline.state,"pending");
   await startAgent(state);
   await expect.poll(async()=>(await getOperation(offline.operation_id)).state,{timeout:20000}).toBe("succeeded");
   // A changed unmanaged field must also invalidate the full configuration revision.
   const revision=(await device()).snapshot.revision;config.custom_setting="local edit";
   const conflict=await submit({kind:"patch_config",expected_revision:revision,set:{qp:30},remove:[],restart_policy:"manual"});
   await expect.poll(async()=>(await getOperation(conflict.operation_id)).state,{timeout:10000}).toBe("failed");
   assert.equal((await getOperation(conflict.operation_id)).result.kind,"conflict");assert.equal(writes,1);
   const read=await submit({kind:"read_config"});
   await expect.poll(async()=>(await getOperation(read.operation_id)).state,{timeout:10000}).toBe("succeeded");
   const current=(await getOperation(read.operation_id)).result.snapshot.revision;
   loseRestartReceipt=true;
   const restart=await submit({kind:"restart",expected_revision:current,administrator_confirmed:true});
   await expect.poll(async()=>(await getOperation(restart.operation_id)).state,{timeout:20000}).toBe("unknown");
   assert.equal(restarts,1);
   await stopAgent();await startAgent(state);
   await expect.poll(async()=>(await getOperation(restart.operation_id)).reconciliation?.kind,{timeout:20000}).toBe("unknown");
   assert.equal(restarts,1);
   const revoked=await page.request.post(base+api+"/revoke",{headers:{"Origin":base,"Sec-Fetch-Site":"same-origin","x-csrf-token":csrf}});assert.equal(revoked.status(),204);
   await expect.poll(()=>agent.exitCode,{timeout:10000}).not.toBe(null);
   assert.equal((await device()).agent_online,false);
   const db=new DatabaseSync(database,{readOnly:true});
   const audited=db.prepare("SELECT count(*) AS count FROM audit_logs WHERE action='operation.reconciled'").get();assert.ok(audited.count>=1);
   assert.equal(db.prepare("SELECT count(*) AS count FROM sqlite_schema WHERE name='hosts'").get().count,0);db.close();
   const log=await readFile(join(root,"agent.log"),"utf8");assert.ok(!log.includes("local-only-password")&&!log.includes(ticket.token));
   await expect(page.locator("body")).not.toContainText("local-only-password");assert.deepEqual(errors,[]);
   console.log("PASS: browser → durable Manager operation → verified WSS → independent Agent → merged local HTTPS save → audit; offline delivery, full-revision conflict, uncertain restart/reconnect and revocation");
  }finally{await stopAgent();await browser.close();for(const socket of tunnels)socket.destroy();await new Promise(done=>ingress.close(done));ingress=undefined;}
 });
}finally{
 await stopAgent();await agentLog?.close();if(sunshine)await new Promise(done=>sunshine.close(done));
 // Only this test-created directory is removed, after every child process has stopped.
 await rm(root,{recursive:true});
}
