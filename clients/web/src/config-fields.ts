export type ConfigField={key:string;label:string;kind:"text"|"integer"|"select";min?:number;max?:number;options?:string[];boolean?:boolean};
export const configFields:ConfigField[]=[
 {key:"sunshine_name",label:"Sunshine 名称",kind:"text"},
 {key:"min_log_level",label:"日志级别",kind:"select",options:["info","warning","error","fatal"]},
 {key:"qp",label:"量化参数 QP",kind:"integer",min:0,max:51},
 {key:"hevc_mode",label:"HEVC 模式",kind:"integer",min:0,max:3},
 {key:"av1_mode",label:"AV1 模式",kind:"integer",min:0,max:3},
 {key:"min_threads",label:"最少线程数",kind:"integer",min:1,max:64},
 {key:"sw_preset",label:"软件编码预设",kind:"select",options:["ultrafast","superfast","veryfast","faster","fast","medium","slow","slower","veryslow"]},
 {key:"nvenc_preset",label:"NVENC 预设",kind:"integer",min:1,max:7},
 {key:"nvenc_vbv_increase",label:"NVENC VBV 增量",kind:"integer",min:0,max:400},
 {key:"nvenc_spatial_aq",label:"NVENC 空间自适应量化",kind:"select",boolean:true,options:["true","false"]},
 {key:"nvenc_h264_cavlc",label:"NVENC H.264 CAVLC",kind:"select",boolean:true,options:["true","false"]},
];
