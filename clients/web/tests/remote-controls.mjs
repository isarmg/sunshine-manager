import assert from "node:assert/strict";
import {chromium,firefox,expect} from "@playwright/test";
import {preview} from "vite";
import {randomUUID} from "node:crypto";
const session={authenticated:true,user_id:"A".repeat(43),username:"admin",role:"admin",csrf_token:"A".repeat(43)};
const server=await preview({preview:{host:"127.0.0.1",port:0,strictPort:true}});
try {for(const engine of [chromium,firefox]){
 const browser=await engine.launch();
 try {
  const page=await browser.newPage();const commands=[];const errors=[];const operations=[];
  page.on("pageerror",e=>errors.push(e.message));
  const device={id:randomUUID(),name:"游戏主机",registered:true,pairing_pending:false,revoked:false,agent_online:true,sunshine_reachable:true,configuration_state:"pending_verification",last_seen_at_micros:Date.now()*1000,capabilities:{restart_allowed:true,managed_fields:["sunshine_name","qp"]},snapshot:{revision:"a".repeat(64),sunshine_version:"2026.516.143833",fields:{sunshine_name:"Original",qp:"28"},effectiveness:"pending_verification"}};
  await page.route("**/api/v2/**",async route=>{
   const req=route.request();const path=new URL(req.url()).pathname;
   if(path.endsWith("/sunshine/devices"))return route.fulfill({json:[device]});
   if(path.endsWith("/tasks")){
    if(req.method()==="POST"){
     const command=req.postDataJSON();commands.push(command);
     assert.equal(req.headers()["x-csrf-token"],session.csrf_token);assert.ok(req.headers()["idempotency-key"]);
     const operation={operation_id:"op_"+randomUUID(),device_id:device.id,action:"sunshine."+command.kind,state:"succeeded",attempt:1,created_at_micros:Date.now()*1000,updated_at_micros:Date.now()*1000,result:{kind:command.kind==="restart"?"restart_acknowledged":"config_saved",snapshot:device.snapshot},reconciliation:null,resolution:null};
     operations.unshift(operation);return route.fulfill({json:operation});
    }return route.fulfill({json:operations});
   }return route.fulfill({json:session});
  });
  await page.goto("http://127.0.0.1:"+server.httpServer.address().port);
  await page.getByRole("button",{name:"Sunshine 配置",exact:true}).click();
  await page.getByLabel(/sunshine_name/).fill("New name");
  await page.getByRole("button",{name:"预览变更",exact:true}).click();
  await expect(page.getByRole("region",{name:"变更差异预览"})).toContainText("Original");
  await expect(page.getByRole("region",{name:"变更差异预览"})).toContainText("New name");
  assert.equal(commands.length,0);
  await page.getByRole("button",{name:"确认保存配置",exact:true}).click();
  await expect.poll(()=>commands.length).toBe(1);
  assert.deepEqual(commands[0],{kind:"patch_config",expected_revision:"a".repeat(64),set:{sunshine_name:"New name"},remove:[],restart_policy:"manual"});
  await page.getByRole("button",{name:"重启 Sunshine",exact:true}).click();
  const dialog=page.getByRole("dialog",{name:"确认重启 Sunshine"});
  await expect(dialog).toContainText("中断正在进行的串流");assert.equal(commands.length,1);
  await dialog.getByRole("button",{name:"Confirm",exact:true}).click();
  await expect.poll(()=>commands.length).toBe(2);
  assert.equal(commands[1].administrator_confirmed,true);
  await page.getByRole("button",{name:"任务记录",exact:true}).click();
  await expect(page.locator("body")).toContainText("restart_acknowledged");
  for(const forbidden of ["应用管理","配对 PIN","日志","诊断","重置显示设备"])await expect(page.getByRole("button",{name:forbidden,exact:true})).toHaveCount(0);
  assert.deepEqual(errors,[]);console.log(engine.name()+": typed Agent patch, diff, manual restart confirmation and task results passed");
 }finally{await browser.close();}
}}finally{await new Promise(done=>server.httpServer.close(done));}
