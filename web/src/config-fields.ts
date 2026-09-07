import { t } from "../shell/i18n.js";
export type ConfigField={key:string;label:string;kind:"text"|"integer"|"select";min?:number;max?:number;options?:string[];boolean?:boolean};
export const configFields:ConfigField[]=[
 {key:"sunshine_name",label:t("Sunshine 名称", "Sunshine name"),kind:"text"},
 {key:"min_log_level",label:t("日志级别", "Log level"),kind:"select",options:["info","warning","error","fatal"]},
 {key:"qp",label:t("量化参数 QP", "Quantization parameter (QP)"),kind:"integer",min:0,max:51},
 {key:"hevc_mode",label:t("HEVC 模式", "HEVC mode"),kind:"integer",min:0,max:3},
 {key:"av1_mode",label:t("AV1 模式", "AV1 mode"),kind:"integer",min:0,max:3},
 {key:"min_threads",label:t("最少线程数", "Minimum threads"),kind:"integer",min:1,max:64},
 {key:"sw_preset",label:t("软件编码预设", "Software encoding preset"),kind:"select",options:["ultrafast","superfast","veryfast","faster","fast","medium","slow","slower","veryslow"]},
 {key:"nvenc_preset",label:t("NVENC 预设", "NVENC preset"),kind:"integer",min:1,max:7},
 {key:"nvenc_vbv_increase",label:t("NVENC VBV 增量", "NVENC VBV increase"),kind:"integer",min:0,max:400},
 {key:"nvenc_spatial_aq",label:t("NVENC 空间自适应量化", "NVENC spatial adaptive quantization"),kind:"select",boolean:true,options:["true","false"]},
 {key:"nvenc_h264_cavlc",label:t("NVENC H.264 CAVLC", "NVENC H.264 CAVLC"),kind:"select",boolean:true,options:["true","false"]},
];
