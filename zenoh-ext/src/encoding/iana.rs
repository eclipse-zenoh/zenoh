//
// Copyright (c) 2024 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
use std::borrow::Cow;
use zenoh::encoding::{Encoding, EncodingMapping, EncodingPrefix};
use zenoh_result::ZResult;

use super::ExtendedEncodingMapping;

/// Encoding mapping used by the [`IanaEncoding`]. It has been generated starting from the
/// MIME types defined by [IANA](https://www.iana.org/assignments/media-types/media-types.xhtml).
/// [`IanaEncodingMapping`] covers the types defined in the `2024-02-16` IANA update.
#[derive(Clone, Copy, Debug)]
pub struct IanaEncodingMapping;

// Extended Encoding Mapping
pub type ExtendedIanaEncodingMapping = ExtendedEncodingMapping<IanaEncodingMapping>;

// Start from 1024 - non-reserved prefixes.
mod prefix {
    use phf::phf_ordered_map;
    use zenoh::encoding::EncodingPrefix;

    pub const APPLICATION_1D_INTERLEAVED_PARITYFEC: EncodingPrefix = 1024;
    pub const APPLICATION_3GPDASH_QOE_REPORT_XML: EncodingPrefix = 1025;
    pub const APPLICATION_3GPP_IMS_XML: EncodingPrefix = 1026;
    pub const APPLICATION_3GPPHAL_JSON: EncodingPrefix = 1027;
    pub const APPLICATION_3GPPHALFORMS_JSON: EncodingPrefix = 1028;
    pub const APPLICATION_A2L: EncodingPrefix = 1029;
    pub const APPLICATION_AML: EncodingPrefix = 1030;
    pub const APPLICATION_ATF: EncodingPrefix = 1031;
    pub const APPLICATION_ATFX: EncodingPrefix = 1032;
    pub const APPLICATION_ATXML: EncodingPrefix = 1033;
    pub const APPLICATION_CALS_1840: EncodingPrefix = 1034;
    pub const APPLICATION_CDFX_XML: EncodingPrefix = 1035;
    pub const APPLICATION_CEA: EncodingPrefix = 1036;
    pub const APPLICATION_CSTADATA_XML: EncodingPrefix = 1037;
    pub const APPLICATION_DCD: EncodingPrefix = 1038;
    pub const APPLICATION_DII: EncodingPrefix = 1039;
    pub const APPLICATION_DIT: EncodingPrefix = 1040;
    pub const APPLICATION_EDI_X12: EncodingPrefix = 1041;
    pub const APPLICATION_EDI_CONSENT: EncodingPrefix = 1042;
    pub const APPLICATION_EDIFACT: EncodingPrefix = 1043;
    pub const APPLICATION_EMERGENCYCALLDATA_COMMENT_XML: EncodingPrefix = 1044;
    pub const APPLICATION_EMERGENCYCALLDATA_CONTROL_XML: EncodingPrefix = 1045;
    pub const APPLICATION_EMERGENCYCALLDATA_DEVICEINFO_XML: EncodingPrefix = 1046;
    pub const APPLICATION_EMERGENCYCALLDATA_LEGACYESN_JSON: EncodingPrefix = 1047;
    pub const APPLICATION_EMERGENCYCALLDATA_PROVIDERINFO_XML: EncodingPrefix = 1048;
    pub const APPLICATION_EMERGENCYCALLDATA_SERVICEINFO_XML: EncodingPrefix = 1049;
    pub const APPLICATION_EMERGENCYCALLDATA_SUBSCRIBERINFO_XML: EncodingPrefix = 1050;
    pub const APPLICATION_EMERGENCYCALLDATA_VEDS_XML: EncodingPrefix = 1051;
    pub const APPLICATION_EMERGENCYCALLDATA_CAP_XML: EncodingPrefix = 1052;
    pub const APPLICATION_EMERGENCYCALLDATA_ECALL_MSD: EncodingPrefix = 1053;
    pub const APPLICATION_H224: EncodingPrefix = 1054;
    pub const APPLICATION_IOTP: EncodingPrefix = 1055;
    pub const APPLICATION_ISUP: EncodingPrefix = 1056;
    pub const APPLICATION_LXF: EncodingPrefix = 1057;
    pub const APPLICATION_MF4: EncodingPrefix = 1058;
    pub const APPLICATION_ODA: EncodingPrefix = 1059;
    pub const APPLICATION_ODX: EncodingPrefix = 1060;
    pub const APPLICATION_PDX: EncodingPrefix = 1061;
    pub const APPLICATION_QSIG: EncodingPrefix = 1062;
    pub const APPLICATION_SGML: EncodingPrefix = 1063;
    pub const APPLICATION_TETRA_ISI: EncodingPrefix = 1064;
    pub const APPLICATION_ACE_CBOR: EncodingPrefix = 1065;
    pub const APPLICATION_ACE_JSON: EncodingPrefix = 1066;
    pub const APPLICATION_ACTIVEMESSAGE: EncodingPrefix = 1067;
    pub const APPLICATION_ACTIVITY_JSON: EncodingPrefix = 1068;
    pub const APPLICATION_AIF_CBOR: EncodingPrefix = 1069;
    pub const APPLICATION_AIF_JSON: EncodingPrefix = 1070;
    pub const APPLICATION_ALTO_CDNI_JSON: EncodingPrefix = 1071;
    pub const APPLICATION_ALTO_CDNIFILTER_JSON: EncodingPrefix = 1072;
    pub const APPLICATION_ALTO_COSTMAP_JSON: EncodingPrefix = 1073;
    pub const APPLICATION_ALTO_COSTMAPFILTER_JSON: EncodingPrefix = 1074;
    pub const APPLICATION_ALTO_DIRECTORY_JSON: EncodingPrefix = 1075;
    pub const APPLICATION_ALTO_ENDPOINTCOST_JSON: EncodingPrefix = 1076;
    pub const APPLICATION_ALTO_ENDPOINTCOSTPARAMS_JSON: EncodingPrefix = 1077;
    pub const APPLICATION_ALTO_ENDPOINTPROP_JSON: EncodingPrefix = 1078;
    pub const APPLICATION_ALTO_ENDPOINTPROPPARAMS_JSON: EncodingPrefix = 1079;
    pub const APPLICATION_ALTO_ERROR_JSON: EncodingPrefix = 1080;
    pub const APPLICATION_ALTO_NETWORKMAP_JSON: EncodingPrefix = 1081;
    pub const APPLICATION_ALTO_NETWORKMAPFILTER_JSON: EncodingPrefix = 1082;
    pub const APPLICATION_ALTO_PROPMAP_JSON: EncodingPrefix = 1083;
    pub const APPLICATION_ALTO_PROPMAPPARAMS_JSON: EncodingPrefix = 1084;
    pub const APPLICATION_ALTO_TIPS_JSON: EncodingPrefix = 1085;
    pub const APPLICATION_ALTO_TIPSPARAMS_JSON: EncodingPrefix = 1086;
    pub const APPLICATION_ALTO_UPDATESTREAMCONTROL_JSON: EncodingPrefix = 1087;
    pub const APPLICATION_ALTO_UPDATESTREAMPARAMS_JSON: EncodingPrefix = 1088;
    pub const APPLICATION_ANDREW_INSET: EncodingPrefix = 1089;
    pub const APPLICATION_APPLEFILE: EncodingPrefix = 1090;
    pub const APPLICATION_AT_JWT: EncodingPrefix = 1091;
    pub const APPLICATION_ATOM_XML: EncodingPrefix = 1092;
    pub const APPLICATION_ATOMCAT_XML: EncodingPrefix = 1093;
    pub const APPLICATION_ATOMDELETED_XML: EncodingPrefix = 1094;
    pub const APPLICATION_ATOMICMAIL: EncodingPrefix = 1095;
    pub const APPLICATION_ATOMSVC_XML: EncodingPrefix = 1096;
    pub const APPLICATION_ATSC_DWD_XML: EncodingPrefix = 1097;
    pub const APPLICATION_ATSC_DYNAMIC_EVENT_MESSAGE: EncodingPrefix = 1098;
    pub const APPLICATION_ATSC_HELD_XML: EncodingPrefix = 1099;
    pub const APPLICATION_ATSC_RDT_JSON: EncodingPrefix = 1100;
    pub const APPLICATION_ATSC_RSAT_XML: EncodingPrefix = 1101;
    pub const APPLICATION_AUTH_POLICY_XML: EncodingPrefix = 1102;
    pub const APPLICATION_AUTOMATIONML_AML_XML: EncodingPrefix = 1103;
    pub const APPLICATION_AUTOMATIONML_AMLX_ZIP: EncodingPrefix = 1104;
    pub const APPLICATION_BACNET_XDD_ZIP: EncodingPrefix = 1105;
    pub const APPLICATION_BATCH_SMTP: EncodingPrefix = 1106;
    pub const APPLICATION_BEEP_XML: EncodingPrefix = 1107;
    pub const APPLICATION_C2PA: EncodingPrefix = 1108;
    pub const APPLICATION_CALENDAR_JSON: EncodingPrefix = 1109;
    pub const APPLICATION_CALENDAR_XML: EncodingPrefix = 1110;
    pub const APPLICATION_CALL_COMPLETION: EncodingPrefix = 1111;
    pub const APPLICATION_CAPTIVE_JSON: EncodingPrefix = 1112;
    pub const APPLICATION_CBOR: EncodingPrefix = 1113;
    pub const APPLICATION_CBOR_SEQ: EncodingPrefix = 1114;
    pub const APPLICATION_CCCEX: EncodingPrefix = 1115;
    pub const APPLICATION_CCMP_XML: EncodingPrefix = 1116;
    pub const APPLICATION_CCXML_XML: EncodingPrefix = 1117;
    pub const APPLICATION_CDA_XML: EncodingPrefix = 1118;
    pub const APPLICATION_CDMI_CAPABILITY: EncodingPrefix = 1119;
    pub const APPLICATION_CDMI_CONTAINER: EncodingPrefix = 1120;
    pub const APPLICATION_CDMI_DOMAIN: EncodingPrefix = 1121;
    pub const APPLICATION_CDMI_OBJECT: EncodingPrefix = 1122;
    pub const APPLICATION_CDMI_QUEUE: EncodingPrefix = 1123;
    pub const APPLICATION_CDNI: EncodingPrefix = 1124;
    pub const APPLICATION_CEA_2018_XML: EncodingPrefix = 1125;
    pub const APPLICATION_CELLML_XML: EncodingPrefix = 1126;
    pub const APPLICATION_CFW: EncodingPrefix = 1127;
    pub const APPLICATION_CID_EDHOC_CBOR_SEQ: EncodingPrefix = 1128;
    pub const APPLICATION_CITY_JSON: EncodingPrefix = 1129;
    pub const APPLICATION_CLR: EncodingPrefix = 1130;
    pub const APPLICATION_CLUE_XML: EncodingPrefix = 1131;
    pub const APPLICATION_CLUE_INFO_XML: EncodingPrefix = 1132;
    pub const APPLICATION_CMS: EncodingPrefix = 1133;
    pub const APPLICATION_CNRP_XML: EncodingPrefix = 1134;
    pub const APPLICATION_COAP_GROUP_JSON: EncodingPrefix = 1135;
    pub const APPLICATION_COAP_PAYLOAD: EncodingPrefix = 1136;
    pub const APPLICATION_COMMONGROUND: EncodingPrefix = 1137;
    pub const APPLICATION_CONCISE_PROBLEM_DETAILS_CBOR: EncodingPrefix = 1138;
    pub const APPLICATION_CONFERENCE_INFO_XML: EncodingPrefix = 1139;
    pub const APPLICATION_COSE: EncodingPrefix = 1140;
    pub const APPLICATION_COSE_KEY: EncodingPrefix = 1141;
    pub const APPLICATION_COSE_KEY_SET: EncodingPrefix = 1142;
    pub const APPLICATION_COSE_X509: EncodingPrefix = 1143;
    pub const APPLICATION_CPL_XML: EncodingPrefix = 1144;
    pub const APPLICATION_CSRATTRS: EncodingPrefix = 1145;
    pub const APPLICATION_CSTA_XML: EncodingPrefix = 1146;
    pub const APPLICATION_CSVM_JSON: EncodingPrefix = 1147;
    pub const APPLICATION_CWL: EncodingPrefix = 1148;
    pub const APPLICATION_CWL_JSON: EncodingPrefix = 1149;
    pub const APPLICATION_CWT: EncodingPrefix = 1150;
    pub const APPLICATION_CYBERCASH: EncodingPrefix = 1151;
    pub const APPLICATION_DASH_XML: EncodingPrefix = 1152;
    pub const APPLICATION_DASH_PATCH_XML: EncodingPrefix = 1153;
    pub const APPLICATION_DASHDELTA: EncodingPrefix = 1154;
    pub const APPLICATION_DAVMOUNT_XML: EncodingPrefix = 1155;
    pub const APPLICATION_DCA_RFT: EncodingPrefix = 1156;
    pub const APPLICATION_DEC_DX: EncodingPrefix = 1157;
    pub const APPLICATION_DIALOG_INFO_XML: EncodingPrefix = 1158;
    pub const APPLICATION_DICOM: EncodingPrefix = 1159;
    pub const APPLICATION_DICOM_JSON: EncodingPrefix = 1160;
    pub const APPLICATION_DICOM_XML: EncodingPrefix = 1161;
    pub const APPLICATION_DNS: EncodingPrefix = 1162;
    pub const APPLICATION_DNS_JSON: EncodingPrefix = 1163;
    pub const APPLICATION_DNS_MESSAGE: EncodingPrefix = 1164;
    pub const APPLICATION_DOTS_CBOR: EncodingPrefix = 1165;
    pub const APPLICATION_DPOP_JWT: EncodingPrefix = 1166;
    pub const APPLICATION_DSKPP_XML: EncodingPrefix = 1167;
    pub const APPLICATION_DSSC_DER: EncodingPrefix = 1168;
    pub const APPLICATION_DSSC_XML: EncodingPrefix = 1169;
    pub const APPLICATION_DVCS: EncodingPrefix = 1170;
    pub const APPLICATION_ECMASCRIPT: EncodingPrefix = 1171;
    pub const APPLICATION_EDHOC_CBOR_SEQ: EncodingPrefix = 1172;
    pub const APPLICATION_EFI: EncodingPrefix = 1173;
    pub const APPLICATION_ELM_JSON: EncodingPrefix = 1174;
    pub const APPLICATION_ELM_XML: EncodingPrefix = 1175;
    pub const APPLICATION_EMMA_XML: EncodingPrefix = 1176;
    pub const APPLICATION_EMOTIONML_XML: EncodingPrefix = 1177;
    pub const APPLICATION_ENCAPRTP: EncodingPrefix = 1178;
    pub const APPLICATION_EPP_XML: EncodingPrefix = 1179;
    pub const APPLICATION_EPUB_ZIP: EncodingPrefix = 1180;
    pub const APPLICATION_ESHOP: EncodingPrefix = 1181;
    pub const APPLICATION_EXAMPLE: EncodingPrefix = 1182;
    pub const APPLICATION_EXI: EncodingPrefix = 1183;
    pub const APPLICATION_EXPECT_CT_REPORT_JSON: EncodingPrefix = 1184;
    pub const APPLICATION_EXPRESS: EncodingPrefix = 1185;
    pub const APPLICATION_FASTINFOSET: EncodingPrefix = 1186;
    pub const APPLICATION_FASTSOAP: EncodingPrefix = 1187;
    pub const APPLICATION_FDF: EncodingPrefix = 1188;
    pub const APPLICATION_FDT_XML: EncodingPrefix = 1189;
    pub const APPLICATION_FHIR_JSON: EncodingPrefix = 1190;
    pub const APPLICATION_FHIR_XML: EncodingPrefix = 1191;
    pub const APPLICATION_FITS: EncodingPrefix = 1192;
    pub const APPLICATION_FLEXFEC: EncodingPrefix = 1193;
    pub const APPLICATION_FONT_SFNT: EncodingPrefix = 1194;
    pub const APPLICATION_FONT_TDPFR: EncodingPrefix = 1195;
    pub const APPLICATION_FONT_WOFF: EncodingPrefix = 1196;
    pub const APPLICATION_FRAMEWORK_ATTRIBUTES_XML: EncodingPrefix = 1197;
    pub const APPLICATION_GEO_JSON: EncodingPrefix = 1198;
    pub const APPLICATION_GEO_JSON_SEQ: EncodingPrefix = 1199;
    pub const APPLICATION_GEOPACKAGE_SQLITE3: EncodingPrefix = 1200;
    pub const APPLICATION_GEOXACML_JSON: EncodingPrefix = 1201;
    pub const APPLICATION_GEOXACML_XML: EncodingPrefix = 1202;
    pub const APPLICATION_GLTF_BUFFER: EncodingPrefix = 1203;
    pub const APPLICATION_GML_XML: EncodingPrefix = 1204;
    pub const APPLICATION_GZIP: EncodingPrefix = 1205;
    pub const APPLICATION_HELD_XML: EncodingPrefix = 1206;
    pub const APPLICATION_HL7V2_XML: EncodingPrefix = 1207;
    pub const APPLICATION_HTTP: EncodingPrefix = 1208;
    pub const APPLICATION_HYPERSTUDIO: EncodingPrefix = 1209;
    pub const APPLICATION_IBE_KEY_REQUEST_XML: EncodingPrefix = 1210;
    pub const APPLICATION_IBE_PKG_REPLY_XML: EncodingPrefix = 1211;
    pub const APPLICATION_IBE_PP_DATA: EncodingPrefix = 1212;
    pub const APPLICATION_IGES: EncodingPrefix = 1213;
    pub const APPLICATION_IM_ISCOMPOSING_XML: EncodingPrefix = 1214;
    pub const APPLICATION_INDEX: EncodingPrefix = 1215;
    pub const APPLICATION_INDEX_CMD: EncodingPrefix = 1216;
    pub const APPLICATION_INDEX_OBJ: EncodingPrefix = 1217;
    pub const APPLICATION_INDEX_RESPONSE: EncodingPrefix = 1218;
    pub const APPLICATION_INDEX_VND: EncodingPrefix = 1219;
    pub const APPLICATION_INKML_XML: EncodingPrefix = 1220;
    pub const APPLICATION_IPFIX: EncodingPrefix = 1221;
    pub const APPLICATION_IPP: EncodingPrefix = 1222;
    pub const APPLICATION_ITS_XML: EncodingPrefix = 1223;
    pub const APPLICATION_JAVA_ARCHIVE: EncodingPrefix = 1224;
    pub const APPLICATION_JAVASCRIPT: EncodingPrefix = 1225;
    pub const APPLICATION_JF2FEED_JSON: EncodingPrefix = 1226;
    pub const APPLICATION_JOSE: EncodingPrefix = 1227;
    pub const APPLICATION_JOSE_JSON: EncodingPrefix = 1228;
    pub const APPLICATION_JRD_JSON: EncodingPrefix = 1229;
    pub const APPLICATION_JSCALENDAR_JSON: EncodingPrefix = 1230;
    pub const APPLICATION_JSCONTACT_JSON: EncodingPrefix = 1231;
    pub const APPLICATION_JSON: EncodingPrefix = 1232;
    pub const APPLICATION_JSON_PATCH_JSON: EncodingPrefix = 1233;
    pub const APPLICATION_JSON_SEQ: EncodingPrefix = 1234;
    pub const APPLICATION_JSONPATH: EncodingPrefix = 1235;
    pub const APPLICATION_JWK_JSON: EncodingPrefix = 1236;
    pub const APPLICATION_JWK_SET_JSON: EncodingPrefix = 1237;
    pub const APPLICATION_JWT: EncodingPrefix = 1238;
    pub const APPLICATION_KPML_REQUEST_XML: EncodingPrefix = 1239;
    pub const APPLICATION_KPML_RESPONSE_XML: EncodingPrefix = 1240;
    pub const APPLICATION_LD_JSON: EncodingPrefix = 1241;
    pub const APPLICATION_LGR_XML: EncodingPrefix = 1242;
    pub const APPLICATION_LINK_FORMAT: EncodingPrefix = 1243;
    pub const APPLICATION_LINKSET: EncodingPrefix = 1244;
    pub const APPLICATION_LINKSET_JSON: EncodingPrefix = 1245;
    pub const APPLICATION_LOAD_CONTROL_XML: EncodingPrefix = 1246;
    pub const APPLICATION_LOGOUT_JWT: EncodingPrefix = 1247;
    pub const APPLICATION_LOST_XML: EncodingPrefix = 1248;
    pub const APPLICATION_LOSTSYNC_XML: EncodingPrefix = 1249;
    pub const APPLICATION_LPF_ZIP: EncodingPrefix = 1250;
    pub const APPLICATION_MAC_BINHEX40: EncodingPrefix = 1251;
    pub const APPLICATION_MACWRITEII: EncodingPrefix = 1252;
    pub const APPLICATION_MADS_XML: EncodingPrefix = 1253;
    pub const APPLICATION_MANIFEST_JSON: EncodingPrefix = 1254;
    pub const APPLICATION_MARC: EncodingPrefix = 1255;
    pub const APPLICATION_MARCXML_XML: EncodingPrefix = 1256;
    pub const APPLICATION_MATHEMATICA: EncodingPrefix = 1257;
    pub const APPLICATION_MATHML_XML: EncodingPrefix = 1258;
    pub const APPLICATION_MATHML_CONTENT_XML: EncodingPrefix = 1259;
    pub const APPLICATION_MATHML_PRESENTATION_XML: EncodingPrefix = 1260;
    pub const APPLICATION_MBMS_ASSOCIATED_PROCEDURE_DESCRIPTION_XML: EncodingPrefix = 1261;
    pub const APPLICATION_MBMS_DEREGISTER_XML: EncodingPrefix = 1262;
    pub const APPLICATION_MBMS_ENVELOPE_XML: EncodingPrefix = 1263;
    pub const APPLICATION_MBMS_MSK_XML: EncodingPrefix = 1264;
    pub const APPLICATION_MBMS_MSK_RESPONSE_XML: EncodingPrefix = 1265;
    pub const APPLICATION_MBMS_PROTECTION_DESCRIPTION_XML: EncodingPrefix = 1266;
    pub const APPLICATION_MBMS_RECEPTION_REPORT_XML: EncodingPrefix = 1267;
    pub const APPLICATION_MBMS_REGISTER_XML: EncodingPrefix = 1268;
    pub const APPLICATION_MBMS_REGISTER_RESPONSE_XML: EncodingPrefix = 1269;
    pub const APPLICATION_MBMS_SCHEDULE_XML: EncodingPrefix = 1270;
    pub const APPLICATION_MBMS_USER_SERVICE_DESCRIPTION_XML: EncodingPrefix = 1271;
    pub const APPLICATION_MBOX: EncodingPrefix = 1272;
    pub const APPLICATION_MEDIA_POLICY_DATASET_XML: EncodingPrefix = 1273;
    pub const APPLICATION_MEDIA_CONTROL_XML: EncodingPrefix = 1274;
    pub const APPLICATION_MEDIASERVERCONTROL_XML: EncodingPrefix = 1275;
    pub const APPLICATION_MERGE_PATCH_JSON: EncodingPrefix = 1276;
    pub const APPLICATION_METALINK4_XML: EncodingPrefix = 1277;
    pub const APPLICATION_METS_XML: EncodingPrefix = 1278;
    pub const APPLICATION_MIKEY: EncodingPrefix = 1279;
    pub const APPLICATION_MIPC: EncodingPrefix = 1280;
    pub const APPLICATION_MISSING_BLOCKS_CBOR_SEQ: EncodingPrefix = 1281;
    pub const APPLICATION_MMT_AEI_XML: EncodingPrefix = 1282;
    pub const APPLICATION_MMT_USD_XML: EncodingPrefix = 1283;
    pub const APPLICATION_MODS_XML: EncodingPrefix = 1284;
    pub const APPLICATION_MOSS_KEYS: EncodingPrefix = 1285;
    pub const APPLICATION_MOSS_SIGNATURE: EncodingPrefix = 1286;
    pub const APPLICATION_MOSSKEY_DATA: EncodingPrefix = 1287;
    pub const APPLICATION_MOSSKEY_REQUEST: EncodingPrefix = 1288;
    pub const APPLICATION_MP21: EncodingPrefix = 1289;
    pub const APPLICATION_MP4: EncodingPrefix = 1290;
    pub const APPLICATION_MPEG4_GENERIC: EncodingPrefix = 1291;
    pub const APPLICATION_MPEG4_IOD: EncodingPrefix = 1292;
    pub const APPLICATION_MPEG4_IOD_XMT: EncodingPrefix = 1293;
    pub const APPLICATION_MRB_CONSUMER_XML: EncodingPrefix = 1294;
    pub const APPLICATION_MRB_PUBLISH_XML: EncodingPrefix = 1295;
    pub const APPLICATION_MSC_IVR_XML: EncodingPrefix = 1296;
    pub const APPLICATION_MSC_MIXER_XML: EncodingPrefix = 1297;
    pub const APPLICATION_MSWORD: EncodingPrefix = 1298;
    pub const APPLICATION_MUD_JSON: EncodingPrefix = 1299;
    pub const APPLICATION_MULTIPART_CORE: EncodingPrefix = 1300;
    pub const APPLICATION_MXF: EncodingPrefix = 1301;
    pub const APPLICATION_N_QUADS: EncodingPrefix = 1302;
    pub const APPLICATION_N_TRIPLES: EncodingPrefix = 1303;
    pub const APPLICATION_NASDATA: EncodingPrefix = 1304;
    pub const APPLICATION_NEWS_CHECKGROUPS: EncodingPrefix = 1305;
    pub const APPLICATION_NEWS_GROUPINFO: EncodingPrefix = 1306;
    pub const APPLICATION_NEWS_TRANSMISSION: EncodingPrefix = 1307;
    pub const APPLICATION_NLSML_XML: EncodingPrefix = 1308;
    pub const APPLICATION_NODE: EncodingPrefix = 1309;
    pub const APPLICATION_NSS: EncodingPrefix = 1310;
    pub const APPLICATION_OAUTH_AUTHZ_REQ_JWT: EncodingPrefix = 1311;
    pub const APPLICATION_OBLIVIOUS_DNS_MESSAGE: EncodingPrefix = 1312;
    pub const APPLICATION_OCSP_REQUEST: EncodingPrefix = 1313;
    pub const APPLICATION_OCSP_RESPONSE: EncodingPrefix = 1314;
    pub const APPLICATION_OCTET_STREAM: EncodingPrefix = 1315;
    pub const APPLICATION_ODM_XML: EncodingPrefix = 1316;
    pub const APPLICATION_OEBPS_PACKAGE_XML: EncodingPrefix = 1317;
    pub const APPLICATION_OGG: EncodingPrefix = 1318;
    pub const APPLICATION_OHTTP_KEYS: EncodingPrefix = 1319;
    pub const APPLICATION_OPC_NODESET_XML: EncodingPrefix = 1320;
    pub const APPLICATION_OSCORE: EncodingPrefix = 1321;
    pub const APPLICATION_OXPS: EncodingPrefix = 1322;
    pub const APPLICATION_P21: EncodingPrefix = 1323;
    pub const APPLICATION_P21_ZIP: EncodingPrefix = 1324;
    pub const APPLICATION_P2P_OVERLAY_XML: EncodingPrefix = 1325;
    pub const APPLICATION_PARITYFEC: EncodingPrefix = 1326;
    pub const APPLICATION_PASSPORT: EncodingPrefix = 1327;
    pub const APPLICATION_PATCH_OPS_ERROR_XML: EncodingPrefix = 1328;
    pub const APPLICATION_PDF: EncodingPrefix = 1329;
    pub const APPLICATION_PEM_CERTIFICATE_CHAIN: EncodingPrefix = 1330;
    pub const APPLICATION_PGP_ENCRYPTED: EncodingPrefix = 1331;
    pub const APPLICATION_PGP_KEYS: EncodingPrefix = 1332;
    pub const APPLICATION_PGP_SIGNATURE: EncodingPrefix = 1333;
    pub const APPLICATION_PIDF_XML: EncodingPrefix = 1334;
    pub const APPLICATION_PIDF_DIFF_XML: EncodingPrefix = 1335;
    pub const APPLICATION_PKCS10: EncodingPrefix = 1336;
    pub const APPLICATION_PKCS12: EncodingPrefix = 1337;
    pub const APPLICATION_PKCS7_MIME: EncodingPrefix = 1338;
    pub const APPLICATION_PKCS7_SIGNATURE: EncodingPrefix = 1339;
    pub const APPLICATION_PKCS8: EncodingPrefix = 1340;
    pub const APPLICATION_PKCS8_ENCRYPTED: EncodingPrefix = 1341;
    pub const APPLICATION_PKIX_ATTR_CERT: EncodingPrefix = 1342;
    pub const APPLICATION_PKIX_CERT: EncodingPrefix = 1343;
    pub const APPLICATION_PKIX_CRL: EncodingPrefix = 1344;
    pub const APPLICATION_PKIX_PKIPATH: EncodingPrefix = 1345;
    pub const APPLICATION_PKIXCMP: EncodingPrefix = 1346;
    pub const APPLICATION_PLS_XML: EncodingPrefix = 1347;
    pub const APPLICATION_POC_SETTINGS_XML: EncodingPrefix = 1348;
    pub const APPLICATION_POSTSCRIPT: EncodingPrefix = 1349;
    pub const APPLICATION_PPSP_TRACKER_JSON: EncodingPrefix = 1350;
    pub const APPLICATION_PRIVATE_TOKEN_ISSUER_DIRECTORY: EncodingPrefix = 1351;
    pub const APPLICATION_PRIVATE_TOKEN_REQUEST: EncodingPrefix = 1352;
    pub const APPLICATION_PRIVATE_TOKEN_RESPONSE: EncodingPrefix = 1353;
    pub const APPLICATION_PROBLEM_JSON: EncodingPrefix = 1354;
    pub const APPLICATION_PROBLEM_XML: EncodingPrefix = 1355;
    pub const APPLICATION_PROVENANCE_XML: EncodingPrefix = 1356;
    pub const APPLICATION_PRS_ALVESTRAND_TITRAX_SHEET: EncodingPrefix = 1357;
    pub const APPLICATION_PRS_CWW: EncodingPrefix = 1358;
    pub const APPLICATION_PRS_CYN: EncodingPrefix = 1359;
    pub const APPLICATION_PRS_HPUB_ZIP: EncodingPrefix = 1360;
    pub const APPLICATION_PRS_IMPLIED_DOCUMENT_XML: EncodingPrefix = 1361;
    pub const APPLICATION_PRS_IMPLIED_EXECUTABLE: EncodingPrefix = 1362;
    pub const APPLICATION_PRS_IMPLIED_OBJECT_JSON: EncodingPrefix = 1363;
    pub const APPLICATION_PRS_IMPLIED_OBJECT_JSON_SEQ: EncodingPrefix = 1364;
    pub const APPLICATION_PRS_IMPLIED_OBJECT_YAML: EncodingPrefix = 1365;
    pub const APPLICATION_PRS_IMPLIED_STRUCTURE: EncodingPrefix = 1366;
    pub const APPLICATION_PRS_NPREND: EncodingPrefix = 1367;
    pub const APPLICATION_PRS_PLUCKER: EncodingPrefix = 1368;
    pub const APPLICATION_PRS_RDF_XML_CRYPT: EncodingPrefix = 1369;
    pub const APPLICATION_PRS_VCFBZIP2: EncodingPrefix = 1370;
    pub const APPLICATION_PRS_XSF_XML: EncodingPrefix = 1371;
    pub const APPLICATION_PSKC_XML: EncodingPrefix = 1372;
    pub const APPLICATION_PVD_JSON: EncodingPrefix = 1373;
    pub const APPLICATION_RAPTORFEC: EncodingPrefix = 1374;
    pub const APPLICATION_RDAP_JSON: EncodingPrefix = 1375;
    pub const APPLICATION_RDF_XML: EncodingPrefix = 1376;
    pub const APPLICATION_REGINFO_XML: EncodingPrefix = 1377;
    pub const APPLICATION_RELAX_NG_COMPACT_SYNTAX: EncodingPrefix = 1378;
    pub const APPLICATION_REMOTE_PRINTING: EncodingPrefix = 1379;
    pub const APPLICATION_REPUTON_JSON: EncodingPrefix = 1380;
    pub const APPLICATION_RESOURCE_LISTS_XML: EncodingPrefix = 1381;
    pub const APPLICATION_RESOURCE_LISTS_DIFF_XML: EncodingPrefix = 1382;
    pub const APPLICATION_RFC_XML: EncodingPrefix = 1383;
    pub const APPLICATION_RISCOS: EncodingPrefix = 1384;
    pub const APPLICATION_RLMI_XML: EncodingPrefix = 1385;
    pub const APPLICATION_RLS_SERVICES_XML: EncodingPrefix = 1386;
    pub const APPLICATION_ROUTE_APD_XML: EncodingPrefix = 1387;
    pub const APPLICATION_ROUTE_S_TSID_XML: EncodingPrefix = 1388;
    pub const APPLICATION_ROUTE_USD_XML: EncodingPrefix = 1389;
    pub const APPLICATION_RPKI_CHECKLIST: EncodingPrefix = 1390;
    pub const APPLICATION_RPKI_GHOSTBUSTERS: EncodingPrefix = 1391;
    pub const APPLICATION_RPKI_MANIFEST: EncodingPrefix = 1392;
    pub const APPLICATION_RPKI_PUBLICATION: EncodingPrefix = 1393;
    pub const APPLICATION_RPKI_ROA: EncodingPrefix = 1394;
    pub const APPLICATION_RPKI_UPDOWN: EncodingPrefix = 1395;
    pub const APPLICATION_RTF: EncodingPrefix = 1396;
    pub const APPLICATION_RTPLOOPBACK: EncodingPrefix = 1397;
    pub const APPLICATION_RTX: EncodingPrefix = 1398;
    pub const APPLICATION_SAMLASSERTION_XML: EncodingPrefix = 1399;
    pub const APPLICATION_SAMLMETADATA_XML: EncodingPrefix = 1400;
    pub const APPLICATION_SARIF_JSON: EncodingPrefix = 1401;
    pub const APPLICATION_SARIF_EXTERNAL_PROPERTIES_JSON: EncodingPrefix = 1402;
    pub const APPLICATION_SBE: EncodingPrefix = 1403;
    pub const APPLICATION_SBML_XML: EncodingPrefix = 1404;
    pub const APPLICATION_SCAIP_XML: EncodingPrefix = 1405;
    pub const APPLICATION_SCIM_JSON: EncodingPrefix = 1406;
    pub const APPLICATION_SCVP_CV_REQUEST: EncodingPrefix = 1407;
    pub const APPLICATION_SCVP_CV_RESPONSE: EncodingPrefix = 1408;
    pub const APPLICATION_SCVP_VP_REQUEST: EncodingPrefix = 1409;
    pub const APPLICATION_SCVP_VP_RESPONSE: EncodingPrefix = 1410;
    pub const APPLICATION_SDP: EncodingPrefix = 1411;
    pub const APPLICATION_SECEVENT_JWT: EncodingPrefix = 1412;
    pub const APPLICATION_SENML_CBOR: EncodingPrefix = 1413;
    pub const APPLICATION_SENML_JSON: EncodingPrefix = 1414;
    pub const APPLICATION_SENML_XML: EncodingPrefix = 1415;
    pub const APPLICATION_SENML_ETCH_CBOR: EncodingPrefix = 1416;
    pub const APPLICATION_SENML_ETCH_JSON: EncodingPrefix = 1417;
    pub const APPLICATION_SENML_EXI: EncodingPrefix = 1418;
    pub const APPLICATION_SENSML_CBOR: EncodingPrefix = 1419;
    pub const APPLICATION_SENSML_JSON: EncodingPrefix = 1420;
    pub const APPLICATION_SENSML_XML: EncodingPrefix = 1421;
    pub const APPLICATION_SENSML_EXI: EncodingPrefix = 1422;
    pub const APPLICATION_SEP_XML: EncodingPrefix = 1423;
    pub const APPLICATION_SEP_EXI: EncodingPrefix = 1424;
    pub const APPLICATION_SESSION_INFO: EncodingPrefix = 1425;
    pub const APPLICATION_SET_PAYMENT: EncodingPrefix = 1426;
    pub const APPLICATION_SET_PAYMENT_INITIATION: EncodingPrefix = 1427;
    pub const APPLICATION_SET_REGISTRATION: EncodingPrefix = 1428;
    pub const APPLICATION_SET_REGISTRATION_INITIATION: EncodingPrefix = 1429;
    pub const APPLICATION_SGML_OPEN_CATALOG: EncodingPrefix = 1430;
    pub const APPLICATION_SHF_XML: EncodingPrefix = 1431;
    pub const APPLICATION_SIEVE: EncodingPrefix = 1432;
    pub const APPLICATION_SIMPLE_FILTER_XML: EncodingPrefix = 1433;
    pub const APPLICATION_SIMPLE_MESSAGE_SUMMARY: EncodingPrefix = 1434;
    pub const APPLICATION_SIMPLESYMBOLCONTAINER: EncodingPrefix = 1435;
    pub const APPLICATION_SIPC: EncodingPrefix = 1436;
    pub const APPLICATION_SLATE: EncodingPrefix = 1437;
    pub const APPLICATION_SMIL: EncodingPrefix = 1438;
    pub const APPLICATION_SMIL_XML: EncodingPrefix = 1439;
    pub const APPLICATION_SMPTE336M: EncodingPrefix = 1440;
    pub const APPLICATION_SOAP_FASTINFOSET: EncodingPrefix = 1441;
    pub const APPLICATION_SOAP_XML: EncodingPrefix = 1442;
    pub const APPLICATION_SPARQL_QUERY: EncodingPrefix = 1443;
    pub const APPLICATION_SPARQL_RESULTS_XML: EncodingPrefix = 1444;
    pub const APPLICATION_SPDX_JSON: EncodingPrefix = 1445;
    pub const APPLICATION_SPIRITS_EVENT_XML: EncodingPrefix = 1446;
    pub const APPLICATION_SQL: EncodingPrefix = 1447;
    pub const APPLICATION_SRGS: EncodingPrefix = 1448;
    pub const APPLICATION_SRGS_XML: EncodingPrefix = 1449;
    pub const APPLICATION_SRU_XML: EncodingPrefix = 1450;
    pub const APPLICATION_SSML_XML: EncodingPrefix = 1451;
    pub const APPLICATION_STIX_JSON: EncodingPrefix = 1452;
    pub const APPLICATION_SWID_CBOR: EncodingPrefix = 1453;
    pub const APPLICATION_SWID_XML: EncodingPrefix = 1454;
    pub const APPLICATION_TAMP_APEX_UPDATE: EncodingPrefix = 1455;
    pub const APPLICATION_TAMP_APEX_UPDATE_CONFIRM: EncodingPrefix = 1456;
    pub const APPLICATION_TAMP_COMMUNITY_UPDATE: EncodingPrefix = 1457;
    pub const APPLICATION_TAMP_COMMUNITY_UPDATE_CONFIRM: EncodingPrefix = 1458;
    pub const APPLICATION_TAMP_ERROR: EncodingPrefix = 1459;
    pub const APPLICATION_TAMP_SEQUENCE_ADJUST: EncodingPrefix = 1460;
    pub const APPLICATION_TAMP_SEQUENCE_ADJUST_CONFIRM: EncodingPrefix = 1461;
    pub const APPLICATION_TAMP_STATUS_QUERY: EncodingPrefix = 1462;
    pub const APPLICATION_TAMP_STATUS_RESPONSE: EncodingPrefix = 1463;
    pub const APPLICATION_TAMP_UPDATE: EncodingPrefix = 1464;
    pub const APPLICATION_TAMP_UPDATE_CONFIRM: EncodingPrefix = 1465;
    pub const APPLICATION_TAXII_JSON: EncodingPrefix = 1466;
    pub const APPLICATION_TD_JSON: EncodingPrefix = 1467;
    pub const APPLICATION_TEI_XML: EncodingPrefix = 1468;
    pub const APPLICATION_THRAUD_XML: EncodingPrefix = 1469;
    pub const APPLICATION_TIMESTAMP_QUERY: EncodingPrefix = 1470;
    pub const APPLICATION_TIMESTAMP_REPLY: EncodingPrefix = 1471;
    pub const APPLICATION_TIMESTAMPED_DATA: EncodingPrefix = 1472;
    pub const APPLICATION_TLSRPT_GZIP: EncodingPrefix = 1473;
    pub const APPLICATION_TLSRPT_JSON: EncodingPrefix = 1474;
    pub const APPLICATION_TM_JSON: EncodingPrefix = 1475;
    pub const APPLICATION_TNAUTHLIST: EncodingPrefix = 1476;
    pub const APPLICATION_TOKEN_INTROSPECTION_JWT: EncodingPrefix = 1477;
    pub const APPLICATION_TRICKLE_ICE_SDPFRAG: EncodingPrefix = 1478;
    pub const APPLICATION_TRIG: EncodingPrefix = 1479;
    pub const APPLICATION_TTML_XML: EncodingPrefix = 1480;
    pub const APPLICATION_TVE_TRIGGER: EncodingPrefix = 1481;
    pub const APPLICATION_TZIF: EncodingPrefix = 1482;
    pub const APPLICATION_TZIF_LEAP: EncodingPrefix = 1483;
    pub const APPLICATION_ULPFEC: EncodingPrefix = 1484;
    pub const APPLICATION_URC_GRPSHEET_XML: EncodingPrefix = 1485;
    pub const APPLICATION_URC_RESSHEET_XML: EncodingPrefix = 1486;
    pub const APPLICATION_URC_TARGETDESC_XML: EncodingPrefix = 1487;
    pub const APPLICATION_URC_UISOCKETDESC_XML: EncodingPrefix = 1488;
    pub const APPLICATION_VCARD_JSON: EncodingPrefix = 1489;
    pub const APPLICATION_VCARD_XML: EncodingPrefix = 1490;
    pub const APPLICATION_VEMMI: EncodingPrefix = 1491;
    pub const APPLICATION_VND_1000MINDS_DECISION_MODEL_XML: EncodingPrefix = 1492;
    pub const APPLICATION_VND_1OB: EncodingPrefix = 1493;
    pub const APPLICATION_VND_3M_POST_IT_NOTES: EncodingPrefix = 1494;
    pub const APPLICATION_VND_3GPP_PROSE_XML: EncodingPrefix = 1495;
    pub const APPLICATION_VND_3GPP_PROSE_PC3A_XML: EncodingPrefix = 1496;
    pub const APPLICATION_VND_3GPP_PROSE_PC3ACH_XML: EncodingPrefix = 1497;
    pub const APPLICATION_VND_3GPP_PROSE_PC3CH_XML: EncodingPrefix = 1498;
    pub const APPLICATION_VND_3GPP_PROSE_PC8_XML: EncodingPrefix = 1499;
    pub const APPLICATION_VND_3GPP_V2X_LOCAL_SERVICE_INFORMATION: EncodingPrefix = 1500;
    pub const APPLICATION_VND_3GPP_5GNAS: EncodingPrefix = 1501;
    pub const APPLICATION_VND_3GPP_GMOP_XML: EncodingPrefix = 1502;
    pub const APPLICATION_VND_3GPP_SRVCC_INFO_XML: EncodingPrefix = 1503;
    pub const APPLICATION_VND_3GPP_ACCESS_TRANSFER_EVENTS_XML: EncodingPrefix = 1504;
    pub const APPLICATION_VND_3GPP_BSF_XML: EncodingPrefix = 1505;
    pub const APPLICATION_VND_3GPP_CRS_XML: EncodingPrefix = 1506;
    pub const APPLICATION_VND_3GPP_CURRENT_LOCATION_DISCOVERY_XML: EncodingPrefix = 1507;
    pub const APPLICATION_VND_3GPP_GTPC: EncodingPrefix = 1508;
    pub const APPLICATION_VND_3GPP_INTERWORKING_DATA: EncodingPrefix = 1509;
    pub const APPLICATION_VND_3GPP_LPP: EncodingPrefix = 1510;
    pub const APPLICATION_VND_3GPP_MC_SIGNALLING_EAR: EncodingPrefix = 1511;
    pub const APPLICATION_VND_3GPP_MCDATA_AFFILIATION_COMMAND_XML: EncodingPrefix = 1512;
    pub const APPLICATION_VND_3GPP_MCDATA_INFO_XML: EncodingPrefix = 1513;
    pub const APPLICATION_VND_3GPP_MCDATA_MSGSTORE_CTRL_REQUEST_XML: EncodingPrefix = 1514;
    pub const APPLICATION_VND_3GPP_MCDATA_PAYLOAD: EncodingPrefix = 1515;
    pub const APPLICATION_VND_3GPP_MCDATA_REGROUP_XML: EncodingPrefix = 1516;
    pub const APPLICATION_VND_3GPP_MCDATA_SERVICE_CONFIG_XML: EncodingPrefix = 1517;
    pub const APPLICATION_VND_3GPP_MCDATA_SIGNALLING: EncodingPrefix = 1518;
    pub const APPLICATION_VND_3GPP_MCDATA_UE_CONFIG_XML: EncodingPrefix = 1519;
    pub const APPLICATION_VND_3GPP_MCDATA_USER_PROFILE_XML: EncodingPrefix = 1520;
    pub const APPLICATION_VND_3GPP_MCPTT_AFFILIATION_COMMAND_XML: EncodingPrefix = 1521;
    pub const APPLICATION_VND_3GPP_MCPTT_FLOOR_REQUEST_XML: EncodingPrefix = 1522;
    pub const APPLICATION_VND_3GPP_MCPTT_INFO_XML: EncodingPrefix = 1523;
    pub const APPLICATION_VND_3GPP_MCPTT_LOCATION_INFO_XML: EncodingPrefix = 1524;
    pub const APPLICATION_VND_3GPP_MCPTT_MBMS_USAGE_INFO_XML: EncodingPrefix = 1525;
    pub const APPLICATION_VND_3GPP_MCPTT_REGROUP_XML: EncodingPrefix = 1526;
    pub const APPLICATION_VND_3GPP_MCPTT_SERVICE_CONFIG_XML: EncodingPrefix = 1527;
    pub const APPLICATION_VND_3GPP_MCPTT_SIGNED_XML: EncodingPrefix = 1528;
    pub const APPLICATION_VND_3GPP_MCPTT_UE_CONFIG_XML: EncodingPrefix = 1529;
    pub const APPLICATION_VND_3GPP_MCPTT_UE_INIT_CONFIG_XML: EncodingPrefix = 1530;
    pub const APPLICATION_VND_3GPP_MCPTT_USER_PROFILE_XML: EncodingPrefix = 1531;
    pub const APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_COMMAND_XML: EncodingPrefix = 1532;
    pub const APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_INFO_XML: EncodingPrefix = 1533;
    pub const APPLICATION_VND_3GPP_MCVIDEO_INFO_XML: EncodingPrefix = 1534;
    pub const APPLICATION_VND_3GPP_MCVIDEO_LOCATION_INFO_XML: EncodingPrefix = 1535;
    pub const APPLICATION_VND_3GPP_MCVIDEO_MBMS_USAGE_INFO_XML: EncodingPrefix = 1536;
    pub const APPLICATION_VND_3GPP_MCVIDEO_REGROUP_XML: EncodingPrefix = 1537;
    pub const APPLICATION_VND_3GPP_MCVIDEO_SERVICE_CONFIG_XML: EncodingPrefix = 1538;
    pub const APPLICATION_VND_3GPP_MCVIDEO_TRANSMISSION_REQUEST_XML: EncodingPrefix = 1539;
    pub const APPLICATION_VND_3GPP_MCVIDEO_UE_CONFIG_XML: EncodingPrefix = 1540;
    pub const APPLICATION_VND_3GPP_MCVIDEO_USER_PROFILE_XML: EncodingPrefix = 1541;
    pub const APPLICATION_VND_3GPP_MID_CALL_XML: EncodingPrefix = 1542;
    pub const APPLICATION_VND_3GPP_NGAP: EncodingPrefix = 1543;
    pub const APPLICATION_VND_3GPP_PFCP: EncodingPrefix = 1544;
    pub const APPLICATION_VND_3GPP_PIC_BW_LARGE: EncodingPrefix = 1545;
    pub const APPLICATION_VND_3GPP_PIC_BW_SMALL: EncodingPrefix = 1546;
    pub const APPLICATION_VND_3GPP_PIC_BW_VAR: EncodingPrefix = 1547;
    pub const APPLICATION_VND_3GPP_S1AP: EncodingPrefix = 1548;
    pub const APPLICATION_VND_3GPP_SEAL_GROUP_DOC_XML: EncodingPrefix = 1549;
    pub const APPLICATION_VND_3GPP_SEAL_INFO_XML: EncodingPrefix = 1550;
    pub const APPLICATION_VND_3GPP_SEAL_LOCATION_INFO_XML: EncodingPrefix = 1551;
    pub const APPLICATION_VND_3GPP_SEAL_MBMS_USAGE_INFO_XML: EncodingPrefix = 1552;
    pub const APPLICATION_VND_3GPP_SEAL_NETWORK_QOS_MANAGEMENT_INFO_XML: EncodingPrefix = 1553;
    pub const APPLICATION_VND_3GPP_SEAL_UE_CONFIG_INFO_XML: EncodingPrefix = 1554;
    pub const APPLICATION_VND_3GPP_SEAL_UNICAST_INFO_XML: EncodingPrefix = 1555;
    pub const APPLICATION_VND_3GPP_SEAL_USER_PROFILE_INFO_XML: EncodingPrefix = 1556;
    pub const APPLICATION_VND_3GPP_SMS: EncodingPrefix = 1557;
    pub const APPLICATION_VND_3GPP_SMS_XML: EncodingPrefix = 1558;
    pub const APPLICATION_VND_3GPP_SRVCC_EXT_XML: EncodingPrefix = 1559;
    pub const APPLICATION_VND_3GPP_STATE_AND_EVENT_INFO_XML: EncodingPrefix = 1560;
    pub const APPLICATION_VND_3GPP_USSD_XML: EncodingPrefix = 1561;
    pub const APPLICATION_VND_3GPP_V2X: EncodingPrefix = 1562;
    pub const APPLICATION_VND_3GPP_VAE_INFO_XML: EncodingPrefix = 1563;
    pub const APPLICATION_VND_3GPP2_BCMCSINFO_XML: EncodingPrefix = 1564;
    pub const APPLICATION_VND_3GPP2_SMS: EncodingPrefix = 1565;
    pub const APPLICATION_VND_3GPP2_TCAP: EncodingPrefix = 1566;
    pub const APPLICATION_VND_3LIGHTSSOFTWARE_IMAGESCAL: EncodingPrefix = 1567;
    pub const APPLICATION_VND_FLOGRAPHIT: EncodingPrefix = 1568;
    pub const APPLICATION_VND_HANDHELD_ENTERTAINMENT_XML: EncodingPrefix = 1569;
    pub const APPLICATION_VND_KINAR: EncodingPrefix = 1570;
    pub const APPLICATION_VND_MFER: EncodingPrefix = 1571;
    pub const APPLICATION_VND_MOBIUS_DAF: EncodingPrefix = 1572;
    pub const APPLICATION_VND_MOBIUS_DIS: EncodingPrefix = 1573;
    pub const APPLICATION_VND_MOBIUS_MBK: EncodingPrefix = 1574;
    pub const APPLICATION_VND_MOBIUS_MQY: EncodingPrefix = 1575;
    pub const APPLICATION_VND_MOBIUS_MSL: EncodingPrefix = 1576;
    pub const APPLICATION_VND_MOBIUS_PLC: EncodingPrefix = 1577;
    pub const APPLICATION_VND_MOBIUS_TXF: EncodingPrefix = 1578;
    pub const APPLICATION_VND_QUARK_QUARKXPRESS: EncodingPrefix = 1579;
    pub const APPLICATION_VND_RENLEARN_RLPRINT: EncodingPrefix = 1580;
    pub const APPLICATION_VND_SIMTECH_MINDMAPPER: EncodingPrefix = 1581;
    pub const APPLICATION_VND_ACCPAC_SIMPLY_ASO: EncodingPrefix = 1582;
    pub const APPLICATION_VND_ACCPAC_SIMPLY_IMP: EncodingPrefix = 1583;
    pub const APPLICATION_VND_ACM_ADDRESSXFER_JSON: EncodingPrefix = 1584;
    pub const APPLICATION_VND_ACM_CHATBOT_JSON: EncodingPrefix = 1585;
    pub const APPLICATION_VND_ACUCOBOL: EncodingPrefix = 1586;
    pub const APPLICATION_VND_ACUCORP: EncodingPrefix = 1587;
    pub const APPLICATION_VND_ADOBE_FLASH_MOVIE: EncodingPrefix = 1588;
    pub const APPLICATION_VND_ADOBE_FORMSCENTRAL_FCDT: EncodingPrefix = 1589;
    pub const APPLICATION_VND_ADOBE_FXP: EncodingPrefix = 1590;
    pub const APPLICATION_VND_ADOBE_PARTIAL_UPLOAD: EncodingPrefix = 1591;
    pub const APPLICATION_VND_ADOBE_XDP_XML: EncodingPrefix = 1592;
    pub const APPLICATION_VND_AETHER_IMP: EncodingPrefix = 1593;
    pub const APPLICATION_VND_AFPC_AFPLINEDATA: EncodingPrefix = 1594;
    pub const APPLICATION_VND_AFPC_AFPLINEDATA_PAGEDEF: EncodingPrefix = 1595;
    pub const APPLICATION_VND_AFPC_CMOCA_CMRESOURCE: EncodingPrefix = 1596;
    pub const APPLICATION_VND_AFPC_FOCA_CHARSET: EncodingPrefix = 1597;
    pub const APPLICATION_VND_AFPC_FOCA_CODEDFONT: EncodingPrefix = 1598;
    pub const APPLICATION_VND_AFPC_FOCA_CODEPAGE: EncodingPrefix = 1599;
    pub const APPLICATION_VND_AFPC_MODCA: EncodingPrefix = 1600;
    pub const APPLICATION_VND_AFPC_MODCA_CMTABLE: EncodingPrefix = 1601;
    pub const APPLICATION_VND_AFPC_MODCA_FORMDEF: EncodingPrefix = 1602;
    pub const APPLICATION_VND_AFPC_MODCA_MEDIUMMAP: EncodingPrefix = 1603;
    pub const APPLICATION_VND_AFPC_MODCA_OBJECTCONTAINER: EncodingPrefix = 1604;
    pub const APPLICATION_VND_AFPC_MODCA_OVERLAY: EncodingPrefix = 1605;
    pub const APPLICATION_VND_AFPC_MODCA_PAGESEGMENT: EncodingPrefix = 1606;
    pub const APPLICATION_VND_AGE: EncodingPrefix = 1607;
    pub const APPLICATION_VND_AH_BARCODE: EncodingPrefix = 1608;
    pub const APPLICATION_VND_AHEAD_SPACE: EncodingPrefix = 1609;
    pub const APPLICATION_VND_AIRZIP_FILESECURE_AZF: EncodingPrefix = 1610;
    pub const APPLICATION_VND_AIRZIP_FILESECURE_AZS: EncodingPrefix = 1611;
    pub const APPLICATION_VND_AMADEUS_JSON: EncodingPrefix = 1612;
    pub const APPLICATION_VND_AMAZON_MOBI8_EBOOK: EncodingPrefix = 1613;
    pub const APPLICATION_VND_AMERICANDYNAMICS_ACC: EncodingPrefix = 1614;
    pub const APPLICATION_VND_AMIGA_AMI: EncodingPrefix = 1615;
    pub const APPLICATION_VND_AMUNDSEN_MAZE_XML: EncodingPrefix = 1616;
    pub const APPLICATION_VND_ANDROID_OTA: EncodingPrefix = 1617;
    pub const APPLICATION_VND_ANKI: EncodingPrefix = 1618;
    pub const APPLICATION_VND_ANSER_WEB_CERTIFICATE_ISSUE_INITIATION: EncodingPrefix = 1619;
    pub const APPLICATION_VND_ANTIX_GAME_COMPONENT: EncodingPrefix = 1620;
    pub const APPLICATION_VND_APACHE_ARROW_FILE: EncodingPrefix = 1621;
    pub const APPLICATION_VND_APACHE_ARROW_STREAM: EncodingPrefix = 1622;
    pub const APPLICATION_VND_APACHE_PARQUET: EncodingPrefix = 1623;
    pub const APPLICATION_VND_APACHE_THRIFT_BINARY: EncodingPrefix = 1624;
    pub const APPLICATION_VND_APACHE_THRIFT_COMPACT: EncodingPrefix = 1625;
    pub const APPLICATION_VND_APACHE_THRIFT_JSON: EncodingPrefix = 1626;
    pub const APPLICATION_VND_APEXLANG: EncodingPrefix = 1627;
    pub const APPLICATION_VND_API_JSON: EncodingPrefix = 1628;
    pub const APPLICATION_VND_APLEXTOR_WARRP_JSON: EncodingPrefix = 1629;
    pub const APPLICATION_VND_APOTHEKENDE_RESERVATION_JSON: EncodingPrefix = 1630;
    pub const APPLICATION_VND_APPLE_INSTALLER_XML: EncodingPrefix = 1631;
    pub const APPLICATION_VND_APPLE_KEYNOTE: EncodingPrefix = 1632;
    pub const APPLICATION_VND_APPLE_MPEGURL: EncodingPrefix = 1633;
    pub const APPLICATION_VND_APPLE_NUMBERS: EncodingPrefix = 1634;
    pub const APPLICATION_VND_APPLE_PAGES: EncodingPrefix = 1635;
    pub const APPLICATION_VND_ARASTRA_SWI: EncodingPrefix = 1636;
    pub const APPLICATION_VND_ARISTANETWORKS_SWI: EncodingPrefix = 1637;
    pub const APPLICATION_VND_ARTISAN_JSON: EncodingPrefix = 1638;
    pub const APPLICATION_VND_ARTSQUARE: EncodingPrefix = 1639;
    pub const APPLICATION_VND_ASTRAEA_SOFTWARE_IOTA: EncodingPrefix = 1640;
    pub const APPLICATION_VND_AUDIOGRAPH: EncodingPrefix = 1641;
    pub const APPLICATION_VND_AUTOPACKAGE: EncodingPrefix = 1642;
    pub const APPLICATION_VND_AVALON_JSON: EncodingPrefix = 1643;
    pub const APPLICATION_VND_AVISTAR_XML: EncodingPrefix = 1644;
    pub const APPLICATION_VND_BALSAMIQ_BMML_XML: EncodingPrefix = 1645;
    pub const APPLICATION_VND_BALSAMIQ_BMPR: EncodingPrefix = 1646;
    pub const APPLICATION_VND_BANANA_ACCOUNTING: EncodingPrefix = 1647;
    pub const APPLICATION_VND_BBF_USP_ERROR: EncodingPrefix = 1648;
    pub const APPLICATION_VND_BBF_USP_MSG: EncodingPrefix = 1649;
    pub const APPLICATION_VND_BBF_USP_MSG_JSON: EncodingPrefix = 1650;
    pub const APPLICATION_VND_BEKITZUR_STECH_JSON: EncodingPrefix = 1651;
    pub const APPLICATION_VND_BELIGHTSOFT_LHZD_ZIP: EncodingPrefix = 1652;
    pub const APPLICATION_VND_BELIGHTSOFT_LHZL_ZIP: EncodingPrefix = 1653;
    pub const APPLICATION_VND_BINT_MED_CONTENT: EncodingPrefix = 1654;
    pub const APPLICATION_VND_BIOPAX_RDF_XML: EncodingPrefix = 1655;
    pub const APPLICATION_VND_BLINK_IDB_VALUE_WRAPPER: EncodingPrefix = 1656;
    pub const APPLICATION_VND_BLUEICE_MULTIPASS: EncodingPrefix = 1657;
    pub const APPLICATION_VND_BLUETOOTH_EP_OOB: EncodingPrefix = 1658;
    pub const APPLICATION_VND_BLUETOOTH_LE_OOB: EncodingPrefix = 1659;
    pub const APPLICATION_VND_BMI: EncodingPrefix = 1660;
    pub const APPLICATION_VND_BPF: EncodingPrefix = 1661;
    pub const APPLICATION_VND_BPF3: EncodingPrefix = 1662;
    pub const APPLICATION_VND_BUSINESSOBJECTS: EncodingPrefix = 1663;
    pub const APPLICATION_VND_BYU_UAPI_JSON: EncodingPrefix = 1664;
    pub const APPLICATION_VND_BZIP3: EncodingPrefix = 1665;
    pub const APPLICATION_VND_CAB_JSCRIPT: EncodingPrefix = 1666;
    pub const APPLICATION_VND_CANON_CPDL: EncodingPrefix = 1667;
    pub const APPLICATION_VND_CANON_LIPS: EncodingPrefix = 1668;
    pub const APPLICATION_VND_CAPASYSTEMS_PG_JSON: EncodingPrefix = 1669;
    pub const APPLICATION_VND_CENDIO_THINLINC_CLIENTCONF: EncodingPrefix = 1670;
    pub const APPLICATION_VND_CENTURY_SYSTEMS_TCP_STREAM: EncodingPrefix = 1671;
    pub const APPLICATION_VND_CHEMDRAW_XML: EncodingPrefix = 1672;
    pub const APPLICATION_VND_CHESS_PGN: EncodingPrefix = 1673;
    pub const APPLICATION_VND_CHIPNUTS_KARAOKE_MMD: EncodingPrefix = 1674;
    pub const APPLICATION_VND_CIEDI: EncodingPrefix = 1675;
    pub const APPLICATION_VND_CINDERELLA: EncodingPrefix = 1676;
    pub const APPLICATION_VND_CIRPACK_ISDN_EXT: EncodingPrefix = 1677;
    pub const APPLICATION_VND_CITATIONSTYLES_STYLE_XML: EncodingPrefix = 1678;
    pub const APPLICATION_VND_CLAYMORE: EncodingPrefix = 1679;
    pub const APPLICATION_VND_CLOANTO_RP9: EncodingPrefix = 1680;
    pub const APPLICATION_VND_CLONK_C4GROUP: EncodingPrefix = 1681;
    pub const APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG: EncodingPrefix = 1682;
    pub const APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG_PKG: EncodingPrefix = 1683;
    pub const APPLICATION_VND_CNCF_HELM_CHART_CONTENT_V1_TAR_GZIP: EncodingPrefix = 1684;
    pub const APPLICATION_VND_CNCF_HELM_CHART_PROVENANCE_V1_PROV: EncodingPrefix = 1685;
    pub const APPLICATION_VND_CNCF_HELM_CONFIG_V1_JSON: EncodingPrefix = 1686;
    pub const APPLICATION_VND_COFFEESCRIPT: EncodingPrefix = 1687;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT: EncodingPrefix = 1688;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT_TEMPLATE: EncodingPrefix = 1689;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION: EncodingPrefix = 1690;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION_TEMPLATE: EncodingPrefix = 1691;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET: EncodingPrefix = 1692;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET_TEMPLATE: EncodingPrefix = 1693;
    pub const APPLICATION_VND_COLLECTION_JSON: EncodingPrefix = 1694;
    pub const APPLICATION_VND_COLLECTION_DOC_JSON: EncodingPrefix = 1695;
    pub const APPLICATION_VND_COLLECTION_NEXT_JSON: EncodingPrefix = 1696;
    pub const APPLICATION_VND_COMICBOOK_ZIP: EncodingPrefix = 1697;
    pub const APPLICATION_VND_COMICBOOK_RAR: EncodingPrefix = 1698;
    pub const APPLICATION_VND_COMMERCE_BATTELLE: EncodingPrefix = 1699;
    pub const APPLICATION_VND_COMMONSPACE: EncodingPrefix = 1700;
    pub const APPLICATION_VND_CONTACT_CMSG: EncodingPrefix = 1701;
    pub const APPLICATION_VND_COREOS_IGNITION_JSON: EncodingPrefix = 1702;
    pub const APPLICATION_VND_COSMOCALLER: EncodingPrefix = 1703;
    pub const APPLICATION_VND_CRICK_CLICKER: EncodingPrefix = 1704;
    pub const APPLICATION_VND_CRICK_CLICKER_KEYBOARD: EncodingPrefix = 1705;
    pub const APPLICATION_VND_CRICK_CLICKER_PALETTE: EncodingPrefix = 1706;
    pub const APPLICATION_VND_CRICK_CLICKER_TEMPLATE: EncodingPrefix = 1707;
    pub const APPLICATION_VND_CRICK_CLICKER_WORDBANK: EncodingPrefix = 1708;
    pub const APPLICATION_VND_CRITICALTOOLS_WBS_XML: EncodingPrefix = 1709;
    pub const APPLICATION_VND_CRYPTII_PIPE_JSON: EncodingPrefix = 1710;
    pub const APPLICATION_VND_CRYPTO_SHADE_FILE: EncodingPrefix = 1711;
    pub const APPLICATION_VND_CRYPTOMATOR_ENCRYPTED: EncodingPrefix = 1712;
    pub const APPLICATION_VND_CRYPTOMATOR_VAULT: EncodingPrefix = 1713;
    pub const APPLICATION_VND_CTC_POSML: EncodingPrefix = 1714;
    pub const APPLICATION_VND_CTCT_WS_XML: EncodingPrefix = 1715;
    pub const APPLICATION_VND_CUPS_PDF: EncodingPrefix = 1716;
    pub const APPLICATION_VND_CUPS_POSTSCRIPT: EncodingPrefix = 1717;
    pub const APPLICATION_VND_CUPS_PPD: EncodingPrefix = 1718;
    pub const APPLICATION_VND_CUPS_RASTER: EncodingPrefix = 1719;
    pub const APPLICATION_VND_CUPS_RAW: EncodingPrefix = 1720;
    pub const APPLICATION_VND_CURL: EncodingPrefix = 1721;
    pub const APPLICATION_VND_CYAN_DEAN_ROOT_XML: EncodingPrefix = 1722;
    pub const APPLICATION_VND_CYBANK: EncodingPrefix = 1723;
    pub const APPLICATION_VND_CYCLONEDX_JSON: EncodingPrefix = 1724;
    pub const APPLICATION_VND_CYCLONEDX_XML: EncodingPrefix = 1725;
    pub const APPLICATION_VND_D2L_COURSEPACKAGE1P0_ZIP: EncodingPrefix = 1726;
    pub const APPLICATION_VND_D3M_DATASET: EncodingPrefix = 1727;
    pub const APPLICATION_VND_D3M_PROBLEM: EncodingPrefix = 1728;
    pub const APPLICATION_VND_DART: EncodingPrefix = 1729;
    pub const APPLICATION_VND_DATA_VISION_RDZ: EncodingPrefix = 1730;
    pub const APPLICATION_VND_DATALOG: EncodingPrefix = 1731;
    pub const APPLICATION_VND_DATAPACKAGE_JSON: EncodingPrefix = 1732;
    pub const APPLICATION_VND_DATARESOURCE_JSON: EncodingPrefix = 1733;
    pub const APPLICATION_VND_DBF: EncodingPrefix = 1734;
    pub const APPLICATION_VND_DEBIAN_BINARY_PACKAGE: EncodingPrefix = 1735;
    pub const APPLICATION_VND_DECE_DATA: EncodingPrefix = 1736;
    pub const APPLICATION_VND_DECE_TTML_XML: EncodingPrefix = 1737;
    pub const APPLICATION_VND_DECE_UNSPECIFIED: EncodingPrefix = 1738;
    pub const APPLICATION_VND_DECE_ZIP: EncodingPrefix = 1739;
    pub const APPLICATION_VND_DENOVO_FCSELAYOUT_LINK: EncodingPrefix = 1740;
    pub const APPLICATION_VND_DESMUME_MOVIE: EncodingPrefix = 1741;
    pub const APPLICATION_VND_DIR_BI_PLATE_DL_NOSUFFIX: EncodingPrefix = 1742;
    pub const APPLICATION_VND_DM_DELEGATION_XML: EncodingPrefix = 1743;
    pub const APPLICATION_VND_DNA: EncodingPrefix = 1744;
    pub const APPLICATION_VND_DOCUMENT_JSON: EncodingPrefix = 1745;
    pub const APPLICATION_VND_DOLBY_MOBILE_1: EncodingPrefix = 1746;
    pub const APPLICATION_VND_DOLBY_MOBILE_2: EncodingPrefix = 1747;
    pub const APPLICATION_VND_DOREMIR_SCORECLOUD_BINARY_DOCUMENT: EncodingPrefix = 1748;
    pub const APPLICATION_VND_DPGRAPH: EncodingPrefix = 1749;
    pub const APPLICATION_VND_DREAMFACTORY: EncodingPrefix = 1750;
    pub const APPLICATION_VND_DRIVE_JSON: EncodingPrefix = 1751;
    pub const APPLICATION_VND_DTG_LOCAL: EncodingPrefix = 1752;
    pub const APPLICATION_VND_DTG_LOCAL_FLASH: EncodingPrefix = 1753;
    pub const APPLICATION_VND_DTG_LOCAL_HTML: EncodingPrefix = 1754;
    pub const APPLICATION_VND_DVB_AIT: EncodingPrefix = 1755;
    pub const APPLICATION_VND_DVB_DVBISL_XML: EncodingPrefix = 1756;
    pub const APPLICATION_VND_DVB_DVBJ: EncodingPrefix = 1757;
    pub const APPLICATION_VND_DVB_ESGCONTAINER: EncodingPrefix = 1758;
    pub const APPLICATION_VND_DVB_IPDCDFTNOTIFACCESS: EncodingPrefix = 1759;
    pub const APPLICATION_VND_DVB_IPDCESGACCESS: EncodingPrefix = 1760;
    pub const APPLICATION_VND_DVB_IPDCESGACCESS2: EncodingPrefix = 1761;
    pub const APPLICATION_VND_DVB_IPDCESGPDD: EncodingPrefix = 1762;
    pub const APPLICATION_VND_DVB_IPDCROAMING: EncodingPrefix = 1763;
    pub const APPLICATION_VND_DVB_IPTV_ALFEC_BASE: EncodingPrefix = 1764;
    pub const APPLICATION_VND_DVB_IPTV_ALFEC_ENHANCEMENT: EncodingPrefix = 1765;
    pub const APPLICATION_VND_DVB_NOTIF_AGGREGATE_ROOT_XML: EncodingPrefix = 1766;
    pub const APPLICATION_VND_DVB_NOTIF_CONTAINER_XML: EncodingPrefix = 1767;
    pub const APPLICATION_VND_DVB_NOTIF_GENERIC_XML: EncodingPrefix = 1768;
    pub const APPLICATION_VND_DVB_NOTIF_IA_MSGLIST_XML: EncodingPrefix = 1769;
    pub const APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_REQUEST_XML: EncodingPrefix = 1770;
    pub const APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_RESPONSE_XML: EncodingPrefix = 1771;
    pub const APPLICATION_VND_DVB_NOTIF_INIT_XML: EncodingPrefix = 1772;
    pub const APPLICATION_VND_DVB_PFR: EncodingPrefix = 1773;
    pub const APPLICATION_VND_DVB_SERVICE: EncodingPrefix = 1774;
    pub const APPLICATION_VND_DXR: EncodingPrefix = 1775;
    pub const APPLICATION_VND_DYNAGEO: EncodingPrefix = 1776;
    pub const APPLICATION_VND_DZR: EncodingPrefix = 1777;
    pub const APPLICATION_VND_EASYKARAOKE_CDGDOWNLOAD: EncodingPrefix = 1778;
    pub const APPLICATION_VND_ECDIS_UPDATE: EncodingPrefix = 1779;
    pub const APPLICATION_VND_ECIP_RLP: EncodingPrefix = 1780;
    pub const APPLICATION_VND_ECLIPSE_DITTO_JSON: EncodingPrefix = 1781;
    pub const APPLICATION_VND_ECOWIN_CHART: EncodingPrefix = 1782;
    pub const APPLICATION_VND_ECOWIN_FILEREQUEST: EncodingPrefix = 1783;
    pub const APPLICATION_VND_ECOWIN_FILEUPDATE: EncodingPrefix = 1784;
    pub const APPLICATION_VND_ECOWIN_SERIES: EncodingPrefix = 1785;
    pub const APPLICATION_VND_ECOWIN_SERIESREQUEST: EncodingPrefix = 1786;
    pub const APPLICATION_VND_ECOWIN_SERIESUPDATE: EncodingPrefix = 1787;
    pub const APPLICATION_VND_EFI_IMG: EncodingPrefix = 1788;
    pub const APPLICATION_VND_EFI_ISO: EncodingPrefix = 1789;
    pub const APPLICATION_VND_ELN_ZIP: EncodingPrefix = 1790;
    pub const APPLICATION_VND_EMCLIENT_ACCESSREQUEST_XML: EncodingPrefix = 1791;
    pub const APPLICATION_VND_ENLIVEN: EncodingPrefix = 1792;
    pub const APPLICATION_VND_ENPHASE_ENVOY: EncodingPrefix = 1793;
    pub const APPLICATION_VND_EPRINTS_DATA_XML: EncodingPrefix = 1794;
    pub const APPLICATION_VND_EPSON_ESF: EncodingPrefix = 1795;
    pub const APPLICATION_VND_EPSON_MSF: EncodingPrefix = 1796;
    pub const APPLICATION_VND_EPSON_QUICKANIME: EncodingPrefix = 1797;
    pub const APPLICATION_VND_EPSON_SALT: EncodingPrefix = 1798;
    pub const APPLICATION_VND_EPSON_SSF: EncodingPrefix = 1799;
    pub const APPLICATION_VND_ERICSSON_QUICKCALL: EncodingPrefix = 1800;
    pub const APPLICATION_VND_EROFS: EncodingPrefix = 1801;
    pub const APPLICATION_VND_ESPASS_ESPASS_ZIP: EncodingPrefix = 1802;
    pub const APPLICATION_VND_ESZIGNO3_XML: EncodingPrefix = 1803;
    pub const APPLICATION_VND_ETSI_AOC_XML: EncodingPrefix = 1804;
    pub const APPLICATION_VND_ETSI_ASIC_E_ZIP: EncodingPrefix = 1805;
    pub const APPLICATION_VND_ETSI_ASIC_S_ZIP: EncodingPrefix = 1806;
    pub const APPLICATION_VND_ETSI_CUG_XML: EncodingPrefix = 1807;
    pub const APPLICATION_VND_ETSI_IPTVCOMMAND_XML: EncodingPrefix = 1808;
    pub const APPLICATION_VND_ETSI_IPTVDISCOVERY_XML: EncodingPrefix = 1809;
    pub const APPLICATION_VND_ETSI_IPTVPROFILE_XML: EncodingPrefix = 1810;
    pub const APPLICATION_VND_ETSI_IPTVSAD_BC_XML: EncodingPrefix = 1811;
    pub const APPLICATION_VND_ETSI_IPTVSAD_COD_XML: EncodingPrefix = 1812;
    pub const APPLICATION_VND_ETSI_IPTVSAD_NPVR_XML: EncodingPrefix = 1813;
    pub const APPLICATION_VND_ETSI_IPTVSERVICE_XML: EncodingPrefix = 1814;
    pub const APPLICATION_VND_ETSI_IPTVSYNC_XML: EncodingPrefix = 1815;
    pub const APPLICATION_VND_ETSI_IPTVUEPROFILE_XML: EncodingPrefix = 1816;
    pub const APPLICATION_VND_ETSI_MCID_XML: EncodingPrefix = 1817;
    pub const APPLICATION_VND_ETSI_MHEG5: EncodingPrefix = 1818;
    pub const APPLICATION_VND_ETSI_OVERLOAD_CONTROL_POLICY_DATASET_XML: EncodingPrefix = 1819;
    pub const APPLICATION_VND_ETSI_PSTN_XML: EncodingPrefix = 1820;
    pub const APPLICATION_VND_ETSI_SCI_XML: EncodingPrefix = 1821;
    pub const APPLICATION_VND_ETSI_SIMSERVS_XML: EncodingPrefix = 1822;
    pub const APPLICATION_VND_ETSI_TIMESTAMP_TOKEN: EncodingPrefix = 1823;
    pub const APPLICATION_VND_ETSI_TSL_XML: EncodingPrefix = 1824;
    pub const APPLICATION_VND_ETSI_TSL_DER: EncodingPrefix = 1825;
    pub const APPLICATION_VND_EU_KASPARIAN_CAR_JSON: EncodingPrefix = 1826;
    pub const APPLICATION_VND_EUDORA_DATA: EncodingPrefix = 1827;
    pub const APPLICATION_VND_EVOLV_ECIG_PROFILE: EncodingPrefix = 1828;
    pub const APPLICATION_VND_EVOLV_ECIG_SETTINGS: EncodingPrefix = 1829;
    pub const APPLICATION_VND_EVOLV_ECIG_THEME: EncodingPrefix = 1830;
    pub const APPLICATION_VND_EXSTREAM_EMPOWER_ZIP: EncodingPrefix = 1831;
    pub const APPLICATION_VND_EXSTREAM_PACKAGE: EncodingPrefix = 1832;
    pub const APPLICATION_VND_EZPIX_ALBUM: EncodingPrefix = 1833;
    pub const APPLICATION_VND_EZPIX_PACKAGE: EncodingPrefix = 1834;
    pub const APPLICATION_VND_F_SECURE_MOBILE: EncodingPrefix = 1835;
    pub const APPLICATION_VND_FAMILYSEARCH_GEDCOM_ZIP: EncodingPrefix = 1836;
    pub const APPLICATION_VND_FASTCOPY_DISK_IMAGE: EncodingPrefix = 1837;
    pub const APPLICATION_VND_FDSN_MSEED: EncodingPrefix = 1838;
    pub const APPLICATION_VND_FDSN_SEED: EncodingPrefix = 1839;
    pub const APPLICATION_VND_FFSNS: EncodingPrefix = 1840;
    pub const APPLICATION_VND_FICLAB_FLB_ZIP: EncodingPrefix = 1841;
    pub const APPLICATION_VND_FILMIT_ZFC: EncodingPrefix = 1842;
    pub const APPLICATION_VND_FINTS: EncodingPrefix = 1843;
    pub const APPLICATION_VND_FIREMONKEYS_CLOUDCELL: EncodingPrefix = 1844;
    pub const APPLICATION_VND_FLUXTIME_CLIP: EncodingPrefix = 1845;
    pub const APPLICATION_VND_FONT_FONTFORGE_SFD: EncodingPrefix = 1846;
    pub const APPLICATION_VND_FRAMEMAKER: EncodingPrefix = 1847;
    pub const APPLICATION_VND_FREELOG_COMIC: EncodingPrefix = 1848;
    pub const APPLICATION_VND_FROGANS_FNC: EncodingPrefix = 1849;
    pub const APPLICATION_VND_FROGANS_LTF: EncodingPrefix = 1850;
    pub const APPLICATION_VND_FSC_WEBLAUNCH: EncodingPrefix = 1851;
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS: EncodingPrefix = 1852;
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_BINDER: EncodingPrefix = 1853;
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_CONTAINER: EncodingPrefix = 1854;
    pub const APPLICATION_VND_FUJIFILM_FB_JFI_XML: EncodingPrefix = 1855;
    pub const APPLICATION_VND_FUJITSU_OASYS: EncodingPrefix = 1856;
    pub const APPLICATION_VND_FUJITSU_OASYS2: EncodingPrefix = 1857;
    pub const APPLICATION_VND_FUJITSU_OASYS3: EncodingPrefix = 1858;
    pub const APPLICATION_VND_FUJITSU_OASYSGP: EncodingPrefix = 1859;
    pub const APPLICATION_VND_FUJITSU_OASYSPRS: EncodingPrefix = 1860;
    pub const APPLICATION_VND_FUJIXEROX_ART_EX: EncodingPrefix = 1861;
    pub const APPLICATION_VND_FUJIXEROX_ART4: EncodingPrefix = 1862;
    pub const APPLICATION_VND_FUJIXEROX_HBPL: EncodingPrefix = 1863;
    pub const APPLICATION_VND_FUJIXEROX_DDD: EncodingPrefix = 1864;
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS: EncodingPrefix = 1865;
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS_BINDER: EncodingPrefix = 1866;
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS_CONTAINER: EncodingPrefix = 1867;
    pub const APPLICATION_VND_FUT_MISNET: EncodingPrefix = 1868;
    pub const APPLICATION_VND_FUTOIN_CBOR: EncodingPrefix = 1869;
    pub const APPLICATION_VND_FUTOIN_JSON: EncodingPrefix = 1870;
    pub const APPLICATION_VND_FUZZYSHEET: EncodingPrefix = 1871;
    pub const APPLICATION_VND_GENOMATIX_TUXEDO: EncodingPrefix = 1872;
    pub const APPLICATION_VND_GENOZIP: EncodingPrefix = 1873;
    pub const APPLICATION_VND_GENTICS_GRD_JSON: EncodingPrefix = 1874;
    pub const APPLICATION_VND_GENTOO_CATMETADATA_XML: EncodingPrefix = 1875;
    pub const APPLICATION_VND_GENTOO_EBUILD: EncodingPrefix = 1876;
    pub const APPLICATION_VND_GENTOO_ECLASS: EncodingPrefix = 1877;
    pub const APPLICATION_VND_GENTOO_GPKG: EncodingPrefix = 1878;
    pub const APPLICATION_VND_GENTOO_MANIFEST: EncodingPrefix = 1879;
    pub const APPLICATION_VND_GENTOO_PKGMETADATA_XML: EncodingPrefix = 1880;
    pub const APPLICATION_VND_GENTOO_XPAK: EncodingPrefix = 1881;
    pub const APPLICATION_VND_GEO_JSON: EncodingPrefix = 1882;
    pub const APPLICATION_VND_GEOCUBE_XML: EncodingPrefix = 1883;
    pub const APPLICATION_VND_GEOGEBRA_FILE: EncodingPrefix = 1884;
    pub const APPLICATION_VND_GEOGEBRA_SLIDES: EncodingPrefix = 1885;
    pub const APPLICATION_VND_GEOGEBRA_TOOL: EncodingPrefix = 1886;
    pub const APPLICATION_VND_GEOMETRY_EXPLORER: EncodingPrefix = 1887;
    pub const APPLICATION_VND_GEONEXT: EncodingPrefix = 1888;
    pub const APPLICATION_VND_GEOPLAN: EncodingPrefix = 1889;
    pub const APPLICATION_VND_GEOSPACE: EncodingPrefix = 1890;
    pub const APPLICATION_VND_GERBER: EncodingPrefix = 1891;
    pub const APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT: EncodingPrefix = 1892;
    pub const APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT_RESPONSE: EncodingPrefix = 1893;
    pub const APPLICATION_VND_GMX: EncodingPrefix = 1894;
    pub const APPLICATION_VND_GNU_TALER_EXCHANGE_JSON: EncodingPrefix = 1895;
    pub const APPLICATION_VND_GNU_TALER_MERCHANT_JSON: EncodingPrefix = 1896;
    pub const APPLICATION_VND_GOOGLE_EARTH_KML_XML: EncodingPrefix = 1897;
    pub const APPLICATION_VND_GOOGLE_EARTH_KMZ: EncodingPrefix = 1898;
    pub const APPLICATION_VND_GOV_SK_E_FORM_XML: EncodingPrefix = 1899;
    pub const APPLICATION_VND_GOV_SK_E_FORM_ZIP: EncodingPrefix = 1900;
    pub const APPLICATION_VND_GOV_SK_XMLDATACONTAINER_XML: EncodingPrefix = 1901;
    pub const APPLICATION_VND_GPXSEE_MAP_XML: EncodingPrefix = 1902;
    pub const APPLICATION_VND_GRAFEQ: EncodingPrefix = 1903;
    pub const APPLICATION_VND_GRIDMP: EncodingPrefix = 1904;
    pub const APPLICATION_VND_GROOVE_ACCOUNT: EncodingPrefix = 1905;
    pub const APPLICATION_VND_GROOVE_HELP: EncodingPrefix = 1906;
    pub const APPLICATION_VND_GROOVE_IDENTITY_MESSAGE: EncodingPrefix = 1907;
    pub const APPLICATION_VND_GROOVE_INJECTOR: EncodingPrefix = 1908;
    pub const APPLICATION_VND_GROOVE_TOOL_MESSAGE: EncodingPrefix = 1909;
    pub const APPLICATION_VND_GROOVE_TOOL_TEMPLATE: EncodingPrefix = 1910;
    pub const APPLICATION_VND_GROOVE_VCARD: EncodingPrefix = 1911;
    pub const APPLICATION_VND_HAL_JSON: EncodingPrefix = 1912;
    pub const APPLICATION_VND_HAL_XML: EncodingPrefix = 1913;
    pub const APPLICATION_VND_HBCI: EncodingPrefix = 1914;
    pub const APPLICATION_VND_HC_JSON: EncodingPrefix = 1915;
    pub const APPLICATION_VND_HCL_BIREPORTS: EncodingPrefix = 1916;
    pub const APPLICATION_VND_HDT: EncodingPrefix = 1917;
    pub const APPLICATION_VND_HEROKU_JSON: EncodingPrefix = 1918;
    pub const APPLICATION_VND_HHE_LESSON_PLAYER: EncodingPrefix = 1919;
    pub const APPLICATION_VND_HP_HPGL: EncodingPrefix = 1920;
    pub const APPLICATION_VND_HP_PCL: EncodingPrefix = 1921;
    pub const APPLICATION_VND_HP_PCLXL: EncodingPrefix = 1922;
    pub const APPLICATION_VND_HP_HPID: EncodingPrefix = 1923;
    pub const APPLICATION_VND_HP_HPS: EncodingPrefix = 1924;
    pub const APPLICATION_VND_HP_JLYT: EncodingPrefix = 1925;
    pub const APPLICATION_VND_HSL: EncodingPrefix = 1926;
    pub const APPLICATION_VND_HTTPHONE: EncodingPrefix = 1927;
    pub const APPLICATION_VND_HYDROSTATIX_SOF_DATA: EncodingPrefix = 1928;
    pub const APPLICATION_VND_HYPER_JSON: EncodingPrefix = 1929;
    pub const APPLICATION_VND_HYPER_ITEM_JSON: EncodingPrefix = 1930;
    pub const APPLICATION_VND_HYPERDRIVE_JSON: EncodingPrefix = 1931;
    pub const APPLICATION_VND_HZN_3D_CROSSWORD: EncodingPrefix = 1932;
    pub const APPLICATION_VND_IBM_MINIPAY: EncodingPrefix = 1933;
    pub const APPLICATION_VND_IBM_AFPLINEDATA: EncodingPrefix = 1934;
    pub const APPLICATION_VND_IBM_ELECTRONIC_MEDIA: EncodingPrefix = 1935;
    pub const APPLICATION_VND_IBM_MODCAP: EncodingPrefix = 1936;
    pub const APPLICATION_VND_IBM_RIGHTS_MANAGEMENT: EncodingPrefix = 1937;
    pub const APPLICATION_VND_IBM_SECURE_CONTAINER: EncodingPrefix = 1938;
    pub const APPLICATION_VND_ICCPROFILE: EncodingPrefix = 1939;
    pub const APPLICATION_VND_IEEE_1905: EncodingPrefix = 1940;
    pub const APPLICATION_VND_IGLOADER: EncodingPrefix = 1941;
    pub const APPLICATION_VND_IMAGEMETER_FOLDER_ZIP: EncodingPrefix = 1942;
    pub const APPLICATION_VND_IMAGEMETER_IMAGE_ZIP: EncodingPrefix = 1943;
    pub const APPLICATION_VND_IMMERVISION_IVP: EncodingPrefix = 1944;
    pub const APPLICATION_VND_IMMERVISION_IVU: EncodingPrefix = 1945;
    pub const APPLICATION_VND_IMS_IMSCCV1P1: EncodingPrefix = 1946;
    pub const APPLICATION_VND_IMS_IMSCCV1P2: EncodingPrefix = 1947;
    pub const APPLICATION_VND_IMS_IMSCCV1P3: EncodingPrefix = 1948;
    pub const APPLICATION_VND_IMS_LIS_V2_RESULT_JSON: EncodingPrefix = 1949;
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLCONSUMERPROFILE_JSON: EncodingPrefix = 1950;
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_JSON: EncodingPrefix = 1951;
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_ID_JSON: EncodingPrefix = 1952;
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_JSON: EncodingPrefix = 1953;
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_SIMPLE_JSON: EncodingPrefix = 1954;
    pub const APPLICATION_VND_INFORMEDCONTROL_RMS_XML: EncodingPrefix = 1955;
    pub const APPLICATION_VND_INFORMIX_VISIONARY: EncodingPrefix = 1956;
    pub const APPLICATION_VND_INFOTECH_PROJECT: EncodingPrefix = 1957;
    pub const APPLICATION_VND_INFOTECH_PROJECT_XML: EncodingPrefix = 1958;
    pub const APPLICATION_VND_INNOPATH_WAMP_NOTIFICATION: EncodingPrefix = 1959;
    pub const APPLICATION_VND_INSORS_IGM: EncodingPrefix = 1960;
    pub const APPLICATION_VND_INTERCON_FORMNET: EncodingPrefix = 1961;
    pub const APPLICATION_VND_INTERGEO: EncodingPrefix = 1962;
    pub const APPLICATION_VND_INTERTRUST_DIGIBOX: EncodingPrefix = 1963;
    pub const APPLICATION_VND_INTERTRUST_NNCP: EncodingPrefix = 1964;
    pub const APPLICATION_VND_INTU_QBO: EncodingPrefix = 1965;
    pub const APPLICATION_VND_INTU_QFX: EncodingPrefix = 1966;
    pub const APPLICATION_VND_IPFS_IPNS_RECORD: EncodingPrefix = 1967;
    pub const APPLICATION_VND_IPLD_CAR: EncodingPrefix = 1968;
    pub const APPLICATION_VND_IPLD_DAG_CBOR: EncodingPrefix = 1969;
    pub const APPLICATION_VND_IPLD_DAG_JSON: EncodingPrefix = 1970;
    pub const APPLICATION_VND_IPLD_RAW: EncodingPrefix = 1971;
    pub const APPLICATION_VND_IPTC_G2_CATALOGITEM_XML: EncodingPrefix = 1972;
    pub const APPLICATION_VND_IPTC_G2_CONCEPTITEM_XML: EncodingPrefix = 1973;
    pub const APPLICATION_VND_IPTC_G2_KNOWLEDGEITEM_XML: EncodingPrefix = 1974;
    pub const APPLICATION_VND_IPTC_G2_NEWSITEM_XML: EncodingPrefix = 1975;
    pub const APPLICATION_VND_IPTC_G2_NEWSMESSAGE_XML: EncodingPrefix = 1976;
    pub const APPLICATION_VND_IPTC_G2_PACKAGEITEM_XML: EncodingPrefix = 1977;
    pub const APPLICATION_VND_IPTC_G2_PLANNINGITEM_XML: EncodingPrefix = 1978;
    pub const APPLICATION_VND_IPUNPLUGGED_RCPROFILE: EncodingPrefix = 1979;
    pub const APPLICATION_VND_IREPOSITORY_PACKAGE_XML: EncodingPrefix = 1980;
    pub const APPLICATION_VND_IS_XPR: EncodingPrefix = 1981;
    pub const APPLICATION_VND_ISAC_FCS: EncodingPrefix = 1982;
    pub const APPLICATION_VND_ISO11783_10_ZIP: EncodingPrefix = 1983;
    pub const APPLICATION_VND_JAM: EncodingPrefix = 1984;
    pub const APPLICATION_VND_JAPANNET_DIRECTORY_SERVICE: EncodingPrefix = 1985;
    pub const APPLICATION_VND_JAPANNET_JPNSTORE_WAKEUP: EncodingPrefix = 1986;
    pub const APPLICATION_VND_JAPANNET_PAYMENT_WAKEUP: EncodingPrefix = 1987;
    pub const APPLICATION_VND_JAPANNET_REGISTRATION: EncodingPrefix = 1988;
    pub const APPLICATION_VND_JAPANNET_REGISTRATION_WAKEUP: EncodingPrefix = 1989;
    pub const APPLICATION_VND_JAPANNET_SETSTORE_WAKEUP: EncodingPrefix = 1990;
    pub const APPLICATION_VND_JAPANNET_VERIFICATION: EncodingPrefix = 1991;
    pub const APPLICATION_VND_JAPANNET_VERIFICATION_WAKEUP: EncodingPrefix = 1992;
    pub const APPLICATION_VND_JCP_JAVAME_MIDLET_RMS: EncodingPrefix = 1993;
    pub const APPLICATION_VND_JISP: EncodingPrefix = 1994;
    pub const APPLICATION_VND_JOOST_JODA_ARCHIVE: EncodingPrefix = 1995;
    pub const APPLICATION_VND_JSK_ISDN_NGN: EncodingPrefix = 1996;
    pub const APPLICATION_VND_KAHOOTZ: EncodingPrefix = 1997;
    pub const APPLICATION_VND_KDE_KARBON: EncodingPrefix = 1998;
    pub const APPLICATION_VND_KDE_KCHART: EncodingPrefix = 1999;
    pub const APPLICATION_VND_KDE_KFORMULA: EncodingPrefix = 2000;
    pub const APPLICATION_VND_KDE_KIVIO: EncodingPrefix = 2001;
    pub const APPLICATION_VND_KDE_KONTOUR: EncodingPrefix = 2002;
    pub const APPLICATION_VND_KDE_KPRESENTER: EncodingPrefix = 2003;
    pub const APPLICATION_VND_KDE_KSPREAD: EncodingPrefix = 2004;
    pub const APPLICATION_VND_KDE_KWORD: EncodingPrefix = 2005;
    pub const APPLICATION_VND_KENAMEAAPP: EncodingPrefix = 2006;
    pub const APPLICATION_VND_KIDSPIRATION: EncodingPrefix = 2007;
    pub const APPLICATION_VND_KOAN: EncodingPrefix = 2008;
    pub const APPLICATION_VND_KODAK_DESCRIPTOR: EncodingPrefix = 2009;
    pub const APPLICATION_VND_LAS: EncodingPrefix = 2010;
    pub const APPLICATION_VND_LAS_LAS_JSON: EncodingPrefix = 2011;
    pub const APPLICATION_VND_LAS_LAS_XML: EncodingPrefix = 2012;
    pub const APPLICATION_VND_LASZIP: EncodingPrefix = 2013;
    pub const APPLICATION_VND_LDEV_PRODUCTLICENSING: EncodingPrefix = 2014;
    pub const APPLICATION_VND_LEAP_JSON: EncodingPrefix = 2015;
    pub const APPLICATION_VND_LIBERTY_REQUEST_XML: EncodingPrefix = 2016;
    pub const APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_DESKTOP: EncodingPrefix = 2017;
    pub const APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_EXCHANGE_XML: EncodingPrefix = 2018;
    pub const APPLICATION_VND_LOGIPIPE_CIRCUIT_ZIP: EncodingPrefix = 2019;
    pub const APPLICATION_VND_LOOM: EncodingPrefix = 2020;
    pub const APPLICATION_VND_LOTUS_1_2_3: EncodingPrefix = 2021;
    pub const APPLICATION_VND_LOTUS_APPROACH: EncodingPrefix = 2022;
    pub const APPLICATION_VND_LOTUS_FREELANCE: EncodingPrefix = 2023;
    pub const APPLICATION_VND_LOTUS_NOTES: EncodingPrefix = 2024;
    pub const APPLICATION_VND_LOTUS_ORGANIZER: EncodingPrefix = 2025;
    pub const APPLICATION_VND_LOTUS_SCREENCAM: EncodingPrefix = 2026;
    pub const APPLICATION_VND_LOTUS_WORDPRO: EncodingPrefix = 2027;
    pub const APPLICATION_VND_MACPORTS_PORTPKG: EncodingPrefix = 2028;
    pub const APPLICATION_VND_MAPBOX_VECTOR_TILE: EncodingPrefix = 2029;
    pub const APPLICATION_VND_MARLIN_DRM_ACTIONTOKEN_XML: EncodingPrefix = 2030;
    pub const APPLICATION_VND_MARLIN_DRM_CONFTOKEN_XML: EncodingPrefix = 2031;
    pub const APPLICATION_VND_MARLIN_DRM_LICENSE_XML: EncodingPrefix = 2032;
    pub const APPLICATION_VND_MARLIN_DRM_MDCF: EncodingPrefix = 2033;
    pub const APPLICATION_VND_MASON_JSON: EncodingPrefix = 2034;
    pub const APPLICATION_VND_MAXAR_ARCHIVE_3TZ_ZIP: EncodingPrefix = 2035;
    pub const APPLICATION_VND_MAXMIND_MAXMIND_DB: EncodingPrefix = 2036;
    pub const APPLICATION_VND_MCD: EncodingPrefix = 2037;
    pub const APPLICATION_VND_MDL: EncodingPrefix = 2038;
    pub const APPLICATION_VND_MDL_MBSDF: EncodingPrefix = 2039;
    pub const APPLICATION_VND_MEDCALCDATA: EncodingPrefix = 2040;
    pub const APPLICATION_VND_MEDIASTATION_CDKEY: EncodingPrefix = 2041;
    pub const APPLICATION_VND_MEDICALHOLODECK_RECORDXR: EncodingPrefix = 2042;
    pub const APPLICATION_VND_MERIDIAN_SLINGSHOT: EncodingPrefix = 2043;
    pub const APPLICATION_VND_MERMAID: EncodingPrefix = 2044;
    pub const APPLICATION_VND_MFMP: EncodingPrefix = 2045;
    pub const APPLICATION_VND_MICRO_JSON: EncodingPrefix = 2046;
    pub const APPLICATION_VND_MICROGRAFX_FLO: EncodingPrefix = 2047;
    pub const APPLICATION_VND_MICROGRAFX_IGX: EncodingPrefix = 2048;
    pub const APPLICATION_VND_MICROSOFT_PORTABLE_EXECUTABLE: EncodingPrefix = 2049;
    pub const APPLICATION_VND_MICROSOFT_WINDOWS_THUMBNAIL_CACHE: EncodingPrefix = 2050;
    pub const APPLICATION_VND_MIELE_JSON: EncodingPrefix = 2051;
    pub const APPLICATION_VND_MIF: EncodingPrefix = 2052;
    pub const APPLICATION_VND_MINISOFT_HP3000_SAVE: EncodingPrefix = 2053;
    pub const APPLICATION_VND_MITSUBISHI_MISTY_GUARD_TRUSTWEB: EncodingPrefix = 2054;
    pub const APPLICATION_VND_MODL: EncodingPrefix = 2055;
    pub const APPLICATION_VND_MOPHUN_APPLICATION: EncodingPrefix = 2056;
    pub const APPLICATION_VND_MOPHUN_CERTIFICATE: EncodingPrefix = 2057;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE: EncodingPrefix = 2058;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_ADSI: EncodingPrefix = 2059;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_FIS: EncodingPrefix = 2060;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_GOTAP: EncodingPrefix = 2061;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_KMR: EncodingPrefix = 2062;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_TTC: EncodingPrefix = 2063;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_WEM: EncodingPrefix = 2064;
    pub const APPLICATION_VND_MOTOROLA_IPRM: EncodingPrefix = 2065;
    pub const APPLICATION_VND_MOZILLA_XUL_XML: EncodingPrefix = 2066;
    pub const APPLICATION_VND_MS_3MFDOCUMENT: EncodingPrefix = 2067;
    pub const APPLICATION_VND_MS_PRINTDEVICECAPABILITIES_XML: EncodingPrefix = 2068;
    pub const APPLICATION_VND_MS_PRINTSCHEMATICKET_XML: EncodingPrefix = 2069;
    pub const APPLICATION_VND_MS_ARTGALRY: EncodingPrefix = 2070;
    pub const APPLICATION_VND_MS_ASF: EncodingPrefix = 2071;
    pub const APPLICATION_VND_MS_CAB_COMPRESSED: EncodingPrefix = 2072;
    pub const APPLICATION_VND_MS_EXCEL: EncodingPrefix = 2073;
    pub const APPLICATION_VND_MS_EXCEL_ADDIN_MACROENABLED_12: EncodingPrefix = 2074;
    pub const APPLICATION_VND_MS_EXCEL_SHEET_BINARY_MACROENABLED_12: EncodingPrefix = 2075;
    pub const APPLICATION_VND_MS_EXCEL_SHEET_MACROENABLED_12: EncodingPrefix = 2076;
    pub const APPLICATION_VND_MS_EXCEL_TEMPLATE_MACROENABLED_12: EncodingPrefix = 2077;
    pub const APPLICATION_VND_MS_FONTOBJECT: EncodingPrefix = 2078;
    pub const APPLICATION_VND_MS_HTMLHELP: EncodingPrefix = 2079;
    pub const APPLICATION_VND_MS_IMS: EncodingPrefix = 2080;
    pub const APPLICATION_VND_MS_LRM: EncodingPrefix = 2081;
    pub const APPLICATION_VND_MS_OFFICE_ACTIVEX_XML: EncodingPrefix = 2082;
    pub const APPLICATION_VND_MS_OFFICETHEME: EncodingPrefix = 2083;
    pub const APPLICATION_VND_MS_PLAYREADY_INITIATOR_XML: EncodingPrefix = 2084;
    pub const APPLICATION_VND_MS_POWERPOINT: EncodingPrefix = 2085;
    pub const APPLICATION_VND_MS_POWERPOINT_ADDIN_MACROENABLED_12: EncodingPrefix = 2086;
    pub const APPLICATION_VND_MS_POWERPOINT_PRESENTATION_MACROENABLED_12: EncodingPrefix = 2087;
    pub const APPLICATION_VND_MS_POWERPOINT_SLIDE_MACROENABLED_12: EncodingPrefix = 2088;
    pub const APPLICATION_VND_MS_POWERPOINT_SLIDESHOW_MACROENABLED_12: EncodingPrefix = 2089;
    pub const APPLICATION_VND_MS_POWERPOINT_TEMPLATE_MACROENABLED_12: EncodingPrefix = 2090;
    pub const APPLICATION_VND_MS_PROJECT: EncodingPrefix = 2091;
    pub const APPLICATION_VND_MS_TNEF: EncodingPrefix = 2092;
    pub const APPLICATION_VND_MS_WINDOWS_DEVICEPAIRING: EncodingPrefix = 2093;
    pub const APPLICATION_VND_MS_WINDOWS_NWPRINTING_OOB: EncodingPrefix = 2094;
    pub const APPLICATION_VND_MS_WINDOWS_PRINTERPAIRING: EncodingPrefix = 2095;
    pub const APPLICATION_VND_MS_WINDOWS_WSD_OOB: EncodingPrefix = 2096;
    pub const APPLICATION_VND_MS_WMDRM_LIC_CHLG_REQ: EncodingPrefix = 2097;
    pub const APPLICATION_VND_MS_WMDRM_LIC_RESP: EncodingPrefix = 2098;
    pub const APPLICATION_VND_MS_WMDRM_METER_CHLG_REQ: EncodingPrefix = 2099;
    pub const APPLICATION_VND_MS_WMDRM_METER_RESP: EncodingPrefix = 2100;
    pub const APPLICATION_VND_MS_WORD_DOCUMENT_MACROENABLED_12: EncodingPrefix = 2101;
    pub const APPLICATION_VND_MS_WORD_TEMPLATE_MACROENABLED_12: EncodingPrefix = 2102;
    pub const APPLICATION_VND_MS_WORKS: EncodingPrefix = 2103;
    pub const APPLICATION_VND_MS_WPL: EncodingPrefix = 2104;
    pub const APPLICATION_VND_MS_XPSDOCUMENT: EncodingPrefix = 2105;
    pub const APPLICATION_VND_MSA_DISK_IMAGE: EncodingPrefix = 2106;
    pub const APPLICATION_VND_MSEQ: EncodingPrefix = 2107;
    pub const APPLICATION_VND_MSIGN: EncodingPrefix = 2108;
    pub const APPLICATION_VND_MULTIAD_CREATOR: EncodingPrefix = 2109;
    pub const APPLICATION_VND_MULTIAD_CREATOR_CIF: EncodingPrefix = 2110;
    pub const APPLICATION_VND_MUSIC_NIFF: EncodingPrefix = 2111;
    pub const APPLICATION_VND_MUSICIAN: EncodingPrefix = 2112;
    pub const APPLICATION_VND_MUVEE_STYLE: EncodingPrefix = 2113;
    pub const APPLICATION_VND_MYNFC: EncodingPrefix = 2114;
    pub const APPLICATION_VND_NACAMAR_YBRID_JSON: EncodingPrefix = 2115;
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_CBOR: EncodingPrefix = 2116;
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_JSON: EncodingPrefix = 2117;
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_XML: EncodingPrefix = 2118;
    pub const APPLICATION_VND_NATO_OPENXMLFORMATS_PACKAGE_IEPD_ZIP: EncodingPrefix = 2119;
    pub const APPLICATION_VND_NCD_CONTROL: EncodingPrefix = 2120;
    pub const APPLICATION_VND_NCD_REFERENCE: EncodingPrefix = 2121;
    pub const APPLICATION_VND_NEARST_INV_JSON: EncodingPrefix = 2122;
    pub const APPLICATION_VND_NEBUMIND_LINE: EncodingPrefix = 2123;
    pub const APPLICATION_VND_NERVANA: EncodingPrefix = 2124;
    pub const APPLICATION_VND_NETFPX: EncodingPrefix = 2125;
    pub const APPLICATION_VND_NEUROLANGUAGE_NLU: EncodingPrefix = 2126;
    pub const APPLICATION_VND_NIMN: EncodingPrefix = 2127;
    pub const APPLICATION_VND_NINTENDO_NITRO_ROM: EncodingPrefix = 2128;
    pub const APPLICATION_VND_NINTENDO_SNES_ROM: EncodingPrefix = 2129;
    pub const APPLICATION_VND_NITF: EncodingPrefix = 2130;
    pub const APPLICATION_VND_NOBLENET_DIRECTORY: EncodingPrefix = 2131;
    pub const APPLICATION_VND_NOBLENET_SEALER: EncodingPrefix = 2132;
    pub const APPLICATION_VND_NOBLENET_WEB: EncodingPrefix = 2133;
    pub const APPLICATION_VND_NOKIA_CATALOGS: EncodingPrefix = 2134;
    pub const APPLICATION_VND_NOKIA_CONML_WBXML: EncodingPrefix = 2135;
    pub const APPLICATION_VND_NOKIA_CONML_XML: EncodingPrefix = 2136;
    pub const APPLICATION_VND_NOKIA_ISDS_RADIO_PRESETS: EncodingPrefix = 2137;
    pub const APPLICATION_VND_NOKIA_IPTV_CONFIG_XML: EncodingPrefix = 2138;
    pub const APPLICATION_VND_NOKIA_LANDMARK_WBXML: EncodingPrefix = 2139;
    pub const APPLICATION_VND_NOKIA_LANDMARK_XML: EncodingPrefix = 2140;
    pub const APPLICATION_VND_NOKIA_LANDMARKCOLLECTION_XML: EncodingPrefix = 2141;
    pub const APPLICATION_VND_NOKIA_N_GAGE_AC_XML: EncodingPrefix = 2142;
    pub const APPLICATION_VND_NOKIA_N_GAGE_DATA: EncodingPrefix = 2143;
    pub const APPLICATION_VND_NOKIA_N_GAGE_SYMBIAN_INSTALL: EncodingPrefix = 2144;
    pub const APPLICATION_VND_NOKIA_NCD: EncodingPrefix = 2145;
    pub const APPLICATION_VND_NOKIA_PCD_WBXML: EncodingPrefix = 2146;
    pub const APPLICATION_VND_NOKIA_PCD_XML: EncodingPrefix = 2147;
    pub const APPLICATION_VND_NOKIA_RADIO_PRESET: EncodingPrefix = 2148;
    pub const APPLICATION_VND_NOKIA_RADIO_PRESETS: EncodingPrefix = 2149;
    pub const APPLICATION_VND_NOVADIGM_EDM: EncodingPrefix = 2150;
    pub const APPLICATION_VND_NOVADIGM_EDX: EncodingPrefix = 2151;
    pub const APPLICATION_VND_NOVADIGM_EXT: EncodingPrefix = 2152;
    pub const APPLICATION_VND_NTT_LOCAL_CONTENT_SHARE: EncodingPrefix = 2153;
    pub const APPLICATION_VND_NTT_LOCAL_FILE_TRANSFER: EncodingPrefix = 2154;
    pub const APPLICATION_VND_NTT_LOCAL_OGW_REMOTE_ACCESS: EncodingPrefix = 2155;
    pub const APPLICATION_VND_NTT_LOCAL_SIP_TA_REMOTE: EncodingPrefix = 2156;
    pub const APPLICATION_VND_NTT_LOCAL_SIP_TA_TCP_STREAM: EncodingPrefix = 2157;
    pub const APPLICATION_VND_OAI_WORKFLOWS: EncodingPrefix = 2158;
    pub const APPLICATION_VND_OAI_WORKFLOWS_JSON: EncodingPrefix = 2159;
    pub const APPLICATION_VND_OAI_WORKFLOWS_YAML: EncodingPrefix = 2160;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_BASE: EncodingPrefix = 2161;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_CHART: EncodingPrefix = 2162;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_CHART_TEMPLATE: EncodingPrefix = 2163;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_DATABASE: EncodingPrefix = 2164;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA: EncodingPrefix = 2165;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA_TEMPLATE: EncodingPrefix = 2166;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS: EncodingPrefix = 2167;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS_TEMPLATE: EncodingPrefix = 2168;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE: EncodingPrefix = 2169;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE_TEMPLATE: EncodingPrefix = 2170;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION: EncodingPrefix = 2171;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION_TEMPLATE: EncodingPrefix = 2172;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET: EncodingPrefix = 2173;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET_TEMPLATE: EncodingPrefix = 2174;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT: EncodingPrefix = 2175;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER: EncodingPrefix = 2176;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER_TEMPLATE: EncodingPrefix = 2177;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_TEMPLATE: EncodingPrefix = 2178;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_WEB: EncodingPrefix = 2179;
    pub const APPLICATION_VND_OBN: EncodingPrefix = 2180;
    pub const APPLICATION_VND_OCF_CBOR: EncodingPrefix = 2181;
    pub const APPLICATION_VND_OCI_IMAGE_MANIFEST_V1_JSON: EncodingPrefix = 2182;
    pub const APPLICATION_VND_OFTN_L10N_JSON: EncodingPrefix = 2183;
    pub const APPLICATION_VND_OIPF_CONTENTACCESSDOWNLOAD_XML: EncodingPrefix = 2184;
    pub const APPLICATION_VND_OIPF_CONTENTACCESSSTREAMING_XML: EncodingPrefix = 2185;
    pub const APPLICATION_VND_OIPF_CSPG_HEXBINARY: EncodingPrefix = 2186;
    pub const APPLICATION_VND_OIPF_DAE_SVG_XML: EncodingPrefix = 2187;
    pub const APPLICATION_VND_OIPF_DAE_XHTML_XML: EncodingPrefix = 2188;
    pub const APPLICATION_VND_OIPF_MIPPVCONTROLMESSAGE_XML: EncodingPrefix = 2189;
    pub const APPLICATION_VND_OIPF_PAE_GEM: EncodingPrefix = 2190;
    pub const APPLICATION_VND_OIPF_SPDISCOVERY_XML: EncodingPrefix = 2191;
    pub const APPLICATION_VND_OIPF_SPDLIST_XML: EncodingPrefix = 2192;
    pub const APPLICATION_VND_OIPF_UEPROFILE_XML: EncodingPrefix = 2193;
    pub const APPLICATION_VND_OIPF_USERPROFILE_XML: EncodingPrefix = 2194;
    pub const APPLICATION_VND_OLPC_SUGAR: EncodingPrefix = 2195;
    pub const APPLICATION_VND_OMA_SCWS_CONFIG: EncodingPrefix = 2196;
    pub const APPLICATION_VND_OMA_SCWS_HTTP_REQUEST: EncodingPrefix = 2197;
    pub const APPLICATION_VND_OMA_SCWS_HTTP_RESPONSE: EncodingPrefix = 2198;
    pub const APPLICATION_VND_OMA_BCAST_ASSOCIATED_PROCEDURE_PARAMETER_XML: EncodingPrefix = 2199;
    pub const APPLICATION_VND_OMA_BCAST_DRM_TRIGGER_XML: EncodingPrefix = 2200;
    pub const APPLICATION_VND_OMA_BCAST_IMD_XML: EncodingPrefix = 2201;
    pub const APPLICATION_VND_OMA_BCAST_LTKM: EncodingPrefix = 2202;
    pub const APPLICATION_VND_OMA_BCAST_NOTIFICATION_XML: EncodingPrefix = 2203;
    pub const APPLICATION_VND_OMA_BCAST_PROVISIONINGTRIGGER: EncodingPrefix = 2204;
    pub const APPLICATION_VND_OMA_BCAST_SGBOOT: EncodingPrefix = 2205;
    pub const APPLICATION_VND_OMA_BCAST_SGDD_XML: EncodingPrefix = 2206;
    pub const APPLICATION_VND_OMA_BCAST_SGDU: EncodingPrefix = 2207;
    pub const APPLICATION_VND_OMA_BCAST_SIMPLE_SYMBOL_CONTAINER: EncodingPrefix = 2208;
    pub const APPLICATION_VND_OMA_BCAST_SMARTCARD_TRIGGER_XML: EncodingPrefix = 2209;
    pub const APPLICATION_VND_OMA_BCAST_SPROV_XML: EncodingPrefix = 2210;
    pub const APPLICATION_VND_OMA_BCAST_STKM: EncodingPrefix = 2211;
    pub const APPLICATION_VND_OMA_CAB_ADDRESS_BOOK_XML: EncodingPrefix = 2212;
    pub const APPLICATION_VND_OMA_CAB_FEATURE_HANDLER_XML: EncodingPrefix = 2213;
    pub const APPLICATION_VND_OMA_CAB_PCC_XML: EncodingPrefix = 2214;
    pub const APPLICATION_VND_OMA_CAB_SUBS_INVITE_XML: EncodingPrefix = 2215;
    pub const APPLICATION_VND_OMA_CAB_USER_PREFS_XML: EncodingPrefix = 2216;
    pub const APPLICATION_VND_OMA_DCD: EncodingPrefix = 2217;
    pub const APPLICATION_VND_OMA_DCDC: EncodingPrefix = 2218;
    pub const APPLICATION_VND_OMA_DD2_XML: EncodingPrefix = 2219;
    pub const APPLICATION_VND_OMA_DRM_RISD_XML: EncodingPrefix = 2220;
    pub const APPLICATION_VND_OMA_GROUP_USAGE_LIST_XML: EncodingPrefix = 2221;
    pub const APPLICATION_VND_OMA_LWM2M_CBOR: EncodingPrefix = 2222;
    pub const APPLICATION_VND_OMA_LWM2M_JSON: EncodingPrefix = 2223;
    pub const APPLICATION_VND_OMA_LWM2M_TLV: EncodingPrefix = 2224;
    pub const APPLICATION_VND_OMA_PAL_XML: EncodingPrefix = 2225;
    pub const APPLICATION_VND_OMA_POC_DETAILED_PROGRESS_REPORT_XML: EncodingPrefix = 2226;
    pub const APPLICATION_VND_OMA_POC_FINAL_REPORT_XML: EncodingPrefix = 2227;
    pub const APPLICATION_VND_OMA_POC_GROUPS_XML: EncodingPrefix = 2228;
    pub const APPLICATION_VND_OMA_POC_INVOCATION_DESCRIPTOR_XML: EncodingPrefix = 2229;
    pub const APPLICATION_VND_OMA_POC_OPTIMIZED_PROGRESS_REPORT_XML: EncodingPrefix = 2230;
    pub const APPLICATION_VND_OMA_PUSH: EncodingPrefix = 2231;
    pub const APPLICATION_VND_OMA_SCIDM_MESSAGES_XML: EncodingPrefix = 2232;
    pub const APPLICATION_VND_OMA_XCAP_DIRECTORY_XML: EncodingPrefix = 2233;
    pub const APPLICATION_VND_OMADS_EMAIL_XML: EncodingPrefix = 2234;
    pub const APPLICATION_VND_OMADS_FILE_XML: EncodingPrefix = 2235;
    pub const APPLICATION_VND_OMADS_FOLDER_XML: EncodingPrefix = 2236;
    pub const APPLICATION_VND_OMALOC_SUPL_INIT: EncodingPrefix = 2237;
    pub const APPLICATION_VND_ONEPAGER: EncodingPrefix = 2238;
    pub const APPLICATION_VND_ONEPAGERTAMP: EncodingPrefix = 2239;
    pub const APPLICATION_VND_ONEPAGERTAMX: EncodingPrefix = 2240;
    pub const APPLICATION_VND_ONEPAGERTAT: EncodingPrefix = 2241;
    pub const APPLICATION_VND_ONEPAGERTATP: EncodingPrefix = 2242;
    pub const APPLICATION_VND_ONEPAGERTATX: EncodingPrefix = 2243;
    pub const APPLICATION_VND_ONVIF_METADATA: EncodingPrefix = 2244;
    pub const APPLICATION_VND_OPENBLOX_GAME_XML: EncodingPrefix = 2245;
    pub const APPLICATION_VND_OPENBLOX_GAME_BINARY: EncodingPrefix = 2246;
    pub const APPLICATION_VND_OPENEYE_OEB: EncodingPrefix = 2247;
    pub const APPLICATION_VND_OPENSTREETMAP_DATA_XML: EncodingPrefix = 2248;
    pub const APPLICATION_VND_OPENTIMESTAMPS_OTS: EncodingPrefix = 2249;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOM_PROPERTIES_XML: EncodingPrefix =
        2250;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOMXMLPROPERTIES_XML:
        EncodingPrefix = 2251;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWING_XML: EncodingPrefix = 2252;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHART_XML: EncodingPrefix =
        2253;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHARTSHAPES_XML:
        EncodingPrefix = 2254;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMCOLORS_XML:
        EncodingPrefix = 2255;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMDATA_XML:
        EncodingPrefix = 2256;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMLAYOUT_XML:
        EncodingPrefix = 2257;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMSTYLE_XML:
        EncodingPrefix = 2258;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_EXTENDED_PROPERTIES_XML:
        EncodingPrefix = 2259;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTAUTHORS_XML:
        EncodingPrefix = 2260;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTS_XML:
        EncodingPrefix = 2261;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_HANDOUTMASTER_XML:
        EncodingPrefix = 2262;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESMASTER_XML:
        EncodingPrefix = 2263;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESSLIDE_XML:
        EncodingPrefix = 2264;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESPROPS_XML:
        EncodingPrefix = 2265;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION:
        EncodingPrefix = 2266;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION_MAIN_XML:
        EncodingPrefix = 2267;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE: EncodingPrefix =
        2268;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE_XML:
        EncodingPrefix = 2269;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDELAYOUT_XML:
        EncodingPrefix = 2270;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEMASTER_XML:
        EncodingPrefix = 2271;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEUPDATEINFO_XML:
        EncodingPrefix = 2272;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW:
        EncodingPrefix = 2273;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW_MAIN_XML:
        EncodingPrefix = 2274;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TABLESTYLES_XML:
        EncodingPrefix = 2275;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TAGS_XML:
        EncodingPrefix = 2276;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE:
        EncodingPrefix = 2277;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE_MAIN_XML:
        EncodingPrefix = 2278;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_VIEWPROPS_XML:
        EncodingPrefix = 2279;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CALCCHAIN_XML:
        EncodingPrefix = 2280;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CHARTSHEET_XML:
        EncodingPrefix = 2281;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_COMMENTS_XML:
        EncodingPrefix = 2282;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CONNECTIONS_XML:
        EncodingPrefix = 2283;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_DIALOGSHEET_XML:
        EncodingPrefix = 2284;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_EXTERNALLINK_XML:
        EncodingPrefix = 2285;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHEDEFINITION_XML: EncodingPrefix = 2286;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHERECORDS_XML:
        EncodingPrefix = 2287;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTTABLE_XML:
        EncodingPrefix = 2288;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_QUERYTABLE_XML:
        EncodingPrefix = 2289;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONHEADERS_XML:
        EncodingPrefix = 2290;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONLOG_XML:
        EncodingPrefix = 2291;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHAREDSTRINGS_XML:
        EncodingPrefix = 2292;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET: EncodingPrefix =
        2293;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET_MAIN_XML:
        EncodingPrefix = 2294;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEETMETADATA_XML:
        EncodingPrefix = 2295;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_STYLES_XML:
        EncodingPrefix = 2296;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLE_XML:
        EncodingPrefix = 2297;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLESINGLECELLS_XML:
        EncodingPrefix = 2298;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE: EncodingPrefix =
        2299;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE_MAIN_XML:
        EncodingPrefix = 2300;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_USERNAMES_XML:
        EncodingPrefix = 2301;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_VOLATILEDEPENDENCIES_XML: EncodingPrefix = 2302;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_WORKSHEET_XML:
        EncodingPrefix = 2303;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEME_XML: EncodingPrefix = 2304;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEMEOVERRIDE_XML: EncodingPrefix =
        2305;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_VMLDRAWING: EncodingPrefix = 2306;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_COMMENTS_XML:
        EncodingPrefix = 2307;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT:
        EncodingPrefix = 2308;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_GLOSSARY_XML: EncodingPrefix = 2309;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_MAIN_XML:
        EncodingPrefix = 2310;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_ENDNOTES_XML:
        EncodingPrefix = 2311;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FONTTABLE_XML:
        EncodingPrefix = 2312;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTER_XML:
        EncodingPrefix = 2313;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTNOTES_XML:
        EncodingPrefix = 2314;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_NUMBERING_XML:
        EncodingPrefix = 2315;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_SETTINGS_XML:
        EncodingPrefix = 2316;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_STYLES_XML:
        EncodingPrefix = 2317;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE:
        EncodingPrefix = 2318;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE_MAIN_XML:
        EncodingPrefix = 2319;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_WEBSETTINGS_XML:
        EncodingPrefix = 2320;
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_CORE_PROPERTIES_XML: EncodingPrefix = 2321;
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_DIGITAL_SIGNATURE_XMLSIGNATURE_XML:
        EncodingPrefix = 2322;
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_RELATIONSHIPS_XML: EncodingPrefix = 2323;
    pub const APPLICATION_VND_ORACLE_RESOURCE_JSON: EncodingPrefix = 2324;
    pub const APPLICATION_VND_ORANGE_INDATA: EncodingPrefix = 2325;
    pub const APPLICATION_VND_OSA_NETDEPLOY: EncodingPrefix = 2326;
    pub const APPLICATION_VND_OSGEO_MAPGUIDE_PACKAGE: EncodingPrefix = 2327;
    pub const APPLICATION_VND_OSGI_BUNDLE: EncodingPrefix = 2328;
    pub const APPLICATION_VND_OSGI_DP: EncodingPrefix = 2329;
    pub const APPLICATION_VND_OSGI_SUBSYSTEM: EncodingPrefix = 2330;
    pub const APPLICATION_VND_OTPS_CT_KIP_XML: EncodingPrefix = 2331;
    pub const APPLICATION_VND_OXLI_COUNTGRAPH: EncodingPrefix = 2332;
    pub const APPLICATION_VND_PAGERDUTY_JSON: EncodingPrefix = 2333;
    pub const APPLICATION_VND_PALM: EncodingPrefix = 2334;
    pub const APPLICATION_VND_PANOPLY: EncodingPrefix = 2335;
    pub const APPLICATION_VND_PAOS_XML: EncodingPrefix = 2336;
    pub const APPLICATION_VND_PATENTDIVE: EncodingPrefix = 2337;
    pub const APPLICATION_VND_PATIENTECOMMSDOC: EncodingPrefix = 2338;
    pub const APPLICATION_VND_PAWAAFILE: EncodingPrefix = 2339;
    pub const APPLICATION_VND_PCOS: EncodingPrefix = 2340;
    pub const APPLICATION_VND_PG_FORMAT: EncodingPrefix = 2341;
    pub const APPLICATION_VND_PG_OSASLI: EncodingPrefix = 2342;
    pub const APPLICATION_VND_PIACCESS_APPLICATION_LICENCE: EncodingPrefix = 2343;
    pub const APPLICATION_VND_PICSEL: EncodingPrefix = 2344;
    pub const APPLICATION_VND_PMI_WIDGET: EncodingPrefix = 2345;
    pub const APPLICATION_VND_POC_GROUP_ADVERTISEMENT_XML: EncodingPrefix = 2346;
    pub const APPLICATION_VND_POCKETLEARN: EncodingPrefix = 2347;
    pub const APPLICATION_VND_POWERBUILDER6: EncodingPrefix = 2348;
    pub const APPLICATION_VND_POWERBUILDER6_S: EncodingPrefix = 2349;
    pub const APPLICATION_VND_POWERBUILDER7: EncodingPrefix = 2350;
    pub const APPLICATION_VND_POWERBUILDER7_S: EncodingPrefix = 2351;
    pub const APPLICATION_VND_POWERBUILDER75: EncodingPrefix = 2352;
    pub const APPLICATION_VND_POWERBUILDER75_S: EncodingPrefix = 2353;
    pub const APPLICATION_VND_PREMINET: EncodingPrefix = 2354;
    pub const APPLICATION_VND_PREVIEWSYSTEMS_BOX: EncodingPrefix = 2355;
    pub const APPLICATION_VND_PROTEUS_MAGAZINE: EncodingPrefix = 2356;
    pub const APPLICATION_VND_PSFS: EncodingPrefix = 2357;
    pub const APPLICATION_VND_PT_MUNDUSMUNDI: EncodingPrefix = 2358;
    pub const APPLICATION_VND_PUBLISHARE_DELTA_TREE: EncodingPrefix = 2359;
    pub const APPLICATION_VND_PVI_PTID1: EncodingPrefix = 2360;
    pub const APPLICATION_VND_PWG_MULTIPLEXED: EncodingPrefix = 2361;
    pub const APPLICATION_VND_PWG_XHTML_PRINT_XML: EncodingPrefix = 2362;
    pub const APPLICATION_VND_QUALCOMM_BREW_APP_RES: EncodingPrefix = 2363;
    pub const APPLICATION_VND_QUARANTAINENET: EncodingPrefix = 2364;
    pub const APPLICATION_VND_QUOBJECT_QUOXDOCUMENT: EncodingPrefix = 2365;
    pub const APPLICATION_VND_RADISYS_MOML_XML: EncodingPrefix = 2366;
    pub const APPLICATION_VND_RADISYS_MSML_XML: EncodingPrefix = 2367;
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_XML: EncodingPrefix = 2368;
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_CONF_XML: EncodingPrefix = 2369;
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_CONN_XML: EncodingPrefix = 2370;
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_DIALOG_XML: EncodingPrefix = 2371;
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_STREAM_XML: EncodingPrefix = 2372;
    pub const APPLICATION_VND_RADISYS_MSML_CONF_XML: EncodingPrefix = 2373;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_XML: EncodingPrefix = 2374;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_BASE_XML: EncodingPrefix = 2375;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_DETECT_XML: EncodingPrefix = 2376;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_SENDRECV_XML: EncodingPrefix = 2377;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_GROUP_XML: EncodingPrefix = 2378;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_SPEECH_XML: EncodingPrefix = 2379;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_TRANSFORM_XML: EncodingPrefix = 2380;
    pub const APPLICATION_VND_RAINSTOR_DATA: EncodingPrefix = 2381;
    pub const APPLICATION_VND_RAPID: EncodingPrefix = 2382;
    pub const APPLICATION_VND_RAR: EncodingPrefix = 2383;
    pub const APPLICATION_VND_REALVNC_BED: EncodingPrefix = 2384;
    pub const APPLICATION_VND_RECORDARE_MUSICXML: EncodingPrefix = 2385;
    pub const APPLICATION_VND_RECORDARE_MUSICXML_XML: EncodingPrefix = 2386;
    pub const APPLICATION_VND_RELPIPE: EncodingPrefix = 2387;
    pub const APPLICATION_VND_RESILIENT_LOGIC: EncodingPrefix = 2388;
    pub const APPLICATION_VND_RESTFUL_JSON: EncodingPrefix = 2389;
    pub const APPLICATION_VND_RIG_CRYPTONOTE: EncodingPrefix = 2390;
    pub const APPLICATION_VND_ROUTE66_LINK66_XML: EncodingPrefix = 2391;
    pub const APPLICATION_VND_RS_274X: EncodingPrefix = 2392;
    pub const APPLICATION_VND_RUCKUS_DOWNLOAD: EncodingPrefix = 2393;
    pub const APPLICATION_VND_S3SMS: EncodingPrefix = 2394;
    pub const APPLICATION_VND_SAILINGTRACKER_TRACK: EncodingPrefix = 2395;
    pub const APPLICATION_VND_SAR: EncodingPrefix = 2396;
    pub const APPLICATION_VND_SBM_CID: EncodingPrefix = 2397;
    pub const APPLICATION_VND_SBM_MID2: EncodingPrefix = 2398;
    pub const APPLICATION_VND_SCRIBUS: EncodingPrefix = 2399;
    pub const APPLICATION_VND_SEALED_3DF: EncodingPrefix = 2400;
    pub const APPLICATION_VND_SEALED_CSF: EncodingPrefix = 2401;
    pub const APPLICATION_VND_SEALED_DOC: EncodingPrefix = 2402;
    pub const APPLICATION_VND_SEALED_EML: EncodingPrefix = 2403;
    pub const APPLICATION_VND_SEALED_MHT: EncodingPrefix = 2404;
    pub const APPLICATION_VND_SEALED_NET: EncodingPrefix = 2405;
    pub const APPLICATION_VND_SEALED_PPT: EncodingPrefix = 2406;
    pub const APPLICATION_VND_SEALED_TIFF: EncodingPrefix = 2407;
    pub const APPLICATION_VND_SEALED_XLS: EncodingPrefix = 2408;
    pub const APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_HTML: EncodingPrefix = 2409;
    pub const APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_PDF: EncodingPrefix = 2410;
    pub const APPLICATION_VND_SEEMAIL: EncodingPrefix = 2411;
    pub const APPLICATION_VND_SEIS_JSON: EncodingPrefix = 2412;
    pub const APPLICATION_VND_SEMA: EncodingPrefix = 2413;
    pub const APPLICATION_VND_SEMD: EncodingPrefix = 2414;
    pub const APPLICATION_VND_SEMF: EncodingPrefix = 2415;
    pub const APPLICATION_VND_SHADE_SAVE_FILE: EncodingPrefix = 2416;
    pub const APPLICATION_VND_SHANA_INFORMED_FORMDATA: EncodingPrefix = 2417;
    pub const APPLICATION_VND_SHANA_INFORMED_FORMTEMPLATE: EncodingPrefix = 2418;
    pub const APPLICATION_VND_SHANA_INFORMED_INTERCHANGE: EncodingPrefix = 2419;
    pub const APPLICATION_VND_SHANA_INFORMED_PACKAGE: EncodingPrefix = 2420;
    pub const APPLICATION_VND_SHOOTPROOF_JSON: EncodingPrefix = 2421;
    pub const APPLICATION_VND_SHOPKICK_JSON: EncodingPrefix = 2422;
    pub const APPLICATION_VND_SHP: EncodingPrefix = 2423;
    pub const APPLICATION_VND_SHX: EncodingPrefix = 2424;
    pub const APPLICATION_VND_SIGROK_SESSION: EncodingPrefix = 2425;
    pub const APPLICATION_VND_SIREN_JSON: EncodingPrefix = 2426;
    pub const APPLICATION_VND_SMAF: EncodingPrefix = 2427;
    pub const APPLICATION_VND_SMART_NOTEBOOK: EncodingPrefix = 2428;
    pub const APPLICATION_VND_SMART_TEACHER: EncodingPrefix = 2429;
    pub const APPLICATION_VND_SMINTIO_PORTALS_ARCHIVE: EncodingPrefix = 2430;
    pub const APPLICATION_VND_SNESDEV_PAGE_TABLE: EncodingPrefix = 2431;
    pub const APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML: EncodingPrefix = 2432;
    pub const APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML_ZIP: EncodingPrefix = 2433;
    pub const APPLICATION_VND_SOLENT_SDKM_XML: EncodingPrefix = 2434;
    pub const APPLICATION_VND_SPOTFIRE_DXP: EncodingPrefix = 2435;
    pub const APPLICATION_VND_SPOTFIRE_SFS: EncodingPrefix = 2436;
    pub const APPLICATION_VND_SQLITE3: EncodingPrefix = 2437;
    pub const APPLICATION_VND_SSS_COD: EncodingPrefix = 2438;
    pub const APPLICATION_VND_SSS_DTF: EncodingPrefix = 2439;
    pub const APPLICATION_VND_SSS_NTF: EncodingPrefix = 2440;
    pub const APPLICATION_VND_STEPMANIA_PACKAGE: EncodingPrefix = 2441;
    pub const APPLICATION_VND_STEPMANIA_STEPCHART: EncodingPrefix = 2442;
    pub const APPLICATION_VND_STREET_STREAM: EncodingPrefix = 2443;
    pub const APPLICATION_VND_SUN_WADL_XML: EncodingPrefix = 2444;
    pub const APPLICATION_VND_SUS_CALENDAR: EncodingPrefix = 2445;
    pub const APPLICATION_VND_SVD: EncodingPrefix = 2446;
    pub const APPLICATION_VND_SWIFTVIEW_ICS: EncodingPrefix = 2447;
    pub const APPLICATION_VND_SYBYL_MOL2: EncodingPrefix = 2448;
    pub const APPLICATION_VND_SYCLE_XML: EncodingPrefix = 2449;
    pub const APPLICATION_VND_SYFT_JSON: EncodingPrefix = 2450;
    pub const APPLICATION_VND_SYNCML_XML: EncodingPrefix = 2451;
    pub const APPLICATION_VND_SYNCML_DM_WBXML: EncodingPrefix = 2452;
    pub const APPLICATION_VND_SYNCML_DM_XML: EncodingPrefix = 2453;
    pub const APPLICATION_VND_SYNCML_DM_NOTIFICATION: EncodingPrefix = 2454;
    pub const APPLICATION_VND_SYNCML_DMDDF_WBXML: EncodingPrefix = 2455;
    pub const APPLICATION_VND_SYNCML_DMDDF_XML: EncodingPrefix = 2456;
    pub const APPLICATION_VND_SYNCML_DMTNDS_WBXML: EncodingPrefix = 2457;
    pub const APPLICATION_VND_SYNCML_DMTNDS_XML: EncodingPrefix = 2458;
    pub const APPLICATION_VND_SYNCML_DS_NOTIFICATION: EncodingPrefix = 2459;
    pub const APPLICATION_VND_TABLESCHEMA_JSON: EncodingPrefix = 2460;
    pub const APPLICATION_VND_TAO_INTENT_MODULE_ARCHIVE: EncodingPrefix = 2461;
    pub const APPLICATION_VND_TCPDUMP_PCAP: EncodingPrefix = 2462;
    pub const APPLICATION_VND_THINK_CELL_PPTTC_JSON: EncodingPrefix = 2463;
    pub const APPLICATION_VND_TMD_MEDIAFLEX_API_XML: EncodingPrefix = 2464;
    pub const APPLICATION_VND_TML: EncodingPrefix = 2465;
    pub const APPLICATION_VND_TMOBILE_LIVETV: EncodingPrefix = 2466;
    pub const APPLICATION_VND_TRI_ONESOURCE: EncodingPrefix = 2467;
    pub const APPLICATION_VND_TRID_TPT: EncodingPrefix = 2468;
    pub const APPLICATION_VND_TRISCAPE_MXS: EncodingPrefix = 2469;
    pub const APPLICATION_VND_TRUEAPP: EncodingPrefix = 2470;
    pub const APPLICATION_VND_TRUEDOC: EncodingPrefix = 2471;
    pub const APPLICATION_VND_UBISOFT_WEBPLAYER: EncodingPrefix = 2472;
    pub const APPLICATION_VND_UFDL: EncodingPrefix = 2473;
    pub const APPLICATION_VND_UIQ_THEME: EncodingPrefix = 2474;
    pub const APPLICATION_VND_UMAJIN: EncodingPrefix = 2475;
    pub const APPLICATION_VND_UNITY: EncodingPrefix = 2476;
    pub const APPLICATION_VND_UOML_XML: EncodingPrefix = 2477;
    pub const APPLICATION_VND_UPLANET_ALERT: EncodingPrefix = 2478;
    pub const APPLICATION_VND_UPLANET_ALERT_WBXML: EncodingPrefix = 2479;
    pub const APPLICATION_VND_UPLANET_BEARER_CHOICE: EncodingPrefix = 2480;
    pub const APPLICATION_VND_UPLANET_BEARER_CHOICE_WBXML: EncodingPrefix = 2481;
    pub const APPLICATION_VND_UPLANET_CACHEOP: EncodingPrefix = 2482;
    pub const APPLICATION_VND_UPLANET_CACHEOP_WBXML: EncodingPrefix = 2483;
    pub const APPLICATION_VND_UPLANET_CHANNEL: EncodingPrefix = 2484;
    pub const APPLICATION_VND_UPLANET_CHANNEL_WBXML: EncodingPrefix = 2485;
    pub const APPLICATION_VND_UPLANET_LIST: EncodingPrefix = 2486;
    pub const APPLICATION_VND_UPLANET_LIST_WBXML: EncodingPrefix = 2487;
    pub const APPLICATION_VND_UPLANET_LISTCMD: EncodingPrefix = 2488;
    pub const APPLICATION_VND_UPLANET_LISTCMD_WBXML: EncodingPrefix = 2489;
    pub const APPLICATION_VND_UPLANET_SIGNAL: EncodingPrefix = 2490;
    pub const APPLICATION_VND_URI_MAP: EncodingPrefix = 2491;
    pub const APPLICATION_VND_VALVE_SOURCE_MATERIAL: EncodingPrefix = 2492;
    pub const APPLICATION_VND_VCX: EncodingPrefix = 2493;
    pub const APPLICATION_VND_VD_STUDY: EncodingPrefix = 2494;
    pub const APPLICATION_VND_VECTORWORKS: EncodingPrefix = 2495;
    pub const APPLICATION_VND_VEL_JSON: EncodingPrefix = 2496;
    pub const APPLICATION_VND_VERIMATRIX_VCAS: EncodingPrefix = 2497;
    pub const APPLICATION_VND_VERITONE_AION_JSON: EncodingPrefix = 2498;
    pub const APPLICATION_VND_VERYANT_THIN: EncodingPrefix = 2499;
    pub const APPLICATION_VND_VES_ENCRYPTED: EncodingPrefix = 2500;
    pub const APPLICATION_VND_VIDSOFT_VIDCONFERENCE: EncodingPrefix = 2501;
    pub const APPLICATION_VND_VISIO: EncodingPrefix = 2502;
    pub const APPLICATION_VND_VISIONARY: EncodingPrefix = 2503;
    pub const APPLICATION_VND_VIVIDENCE_SCRIPTFILE: EncodingPrefix = 2504;
    pub const APPLICATION_VND_VSF: EncodingPrefix = 2505;
    pub const APPLICATION_VND_WAP_SIC: EncodingPrefix = 2506;
    pub const APPLICATION_VND_WAP_SLC: EncodingPrefix = 2507;
    pub const APPLICATION_VND_WAP_WBXML: EncodingPrefix = 2508;
    pub const APPLICATION_VND_WAP_WMLC: EncodingPrefix = 2509;
    pub const APPLICATION_VND_WAP_WMLSCRIPTC: EncodingPrefix = 2510;
    pub const APPLICATION_VND_WASMFLOW_WAFL: EncodingPrefix = 2511;
    pub const APPLICATION_VND_WEBTURBO: EncodingPrefix = 2512;
    pub const APPLICATION_VND_WFA_DPP: EncodingPrefix = 2513;
    pub const APPLICATION_VND_WFA_P2P: EncodingPrefix = 2514;
    pub const APPLICATION_VND_WFA_WSC: EncodingPrefix = 2515;
    pub const APPLICATION_VND_WINDOWS_DEVICEPAIRING: EncodingPrefix = 2516;
    pub const APPLICATION_VND_WMC: EncodingPrefix = 2517;
    pub const APPLICATION_VND_WMF_BOOTSTRAP: EncodingPrefix = 2518;
    pub const APPLICATION_VND_WOLFRAM_MATHEMATICA: EncodingPrefix = 2519;
    pub const APPLICATION_VND_WOLFRAM_MATHEMATICA_PACKAGE: EncodingPrefix = 2520;
    pub const APPLICATION_VND_WOLFRAM_PLAYER: EncodingPrefix = 2521;
    pub const APPLICATION_VND_WORDLIFT: EncodingPrefix = 2522;
    pub const APPLICATION_VND_WORDPERFECT: EncodingPrefix = 2523;
    pub const APPLICATION_VND_WQD: EncodingPrefix = 2524;
    pub const APPLICATION_VND_WRQ_HP3000_LABELLED: EncodingPrefix = 2525;
    pub const APPLICATION_VND_WT_STF: EncodingPrefix = 2526;
    pub const APPLICATION_VND_WV_CSP_WBXML: EncodingPrefix = 2527;
    pub const APPLICATION_VND_WV_CSP_XML: EncodingPrefix = 2528;
    pub const APPLICATION_VND_WV_SSP_XML: EncodingPrefix = 2529;
    pub const APPLICATION_VND_XACML_JSON: EncodingPrefix = 2530;
    pub const APPLICATION_VND_XARA: EncodingPrefix = 2531;
    pub const APPLICATION_VND_XECRETS_ENCRYPTED: EncodingPrefix = 2532;
    pub const APPLICATION_VND_XFDL: EncodingPrefix = 2533;
    pub const APPLICATION_VND_XFDL_WEBFORM: EncodingPrefix = 2534;
    pub const APPLICATION_VND_XMI_XML: EncodingPrefix = 2535;
    pub const APPLICATION_VND_XMPIE_CPKG: EncodingPrefix = 2536;
    pub const APPLICATION_VND_XMPIE_DPKG: EncodingPrefix = 2537;
    pub const APPLICATION_VND_XMPIE_PLAN: EncodingPrefix = 2538;
    pub const APPLICATION_VND_XMPIE_PPKG: EncodingPrefix = 2539;
    pub const APPLICATION_VND_XMPIE_XLIM: EncodingPrefix = 2540;
    pub const APPLICATION_VND_YAMAHA_HV_DIC: EncodingPrefix = 2541;
    pub const APPLICATION_VND_YAMAHA_HV_SCRIPT: EncodingPrefix = 2542;
    pub const APPLICATION_VND_YAMAHA_HV_VOICE: EncodingPrefix = 2543;
    pub const APPLICATION_VND_YAMAHA_OPENSCOREFORMAT: EncodingPrefix = 2544;
    pub const APPLICATION_VND_YAMAHA_OPENSCOREFORMAT_OSFPVG_XML: EncodingPrefix = 2545;
    pub const APPLICATION_VND_YAMAHA_REMOTE_SETUP: EncodingPrefix = 2546;
    pub const APPLICATION_VND_YAMAHA_SMAF_AUDIO: EncodingPrefix = 2547;
    pub const APPLICATION_VND_YAMAHA_SMAF_PHRASE: EncodingPrefix = 2548;
    pub const APPLICATION_VND_YAMAHA_THROUGH_NGN: EncodingPrefix = 2549;
    pub const APPLICATION_VND_YAMAHA_TUNNEL_UDPENCAP: EncodingPrefix = 2550;
    pub const APPLICATION_VND_YAOWEME: EncodingPrefix = 2551;
    pub const APPLICATION_VND_YELLOWRIVER_CUSTOM_MENU: EncodingPrefix = 2552;
    pub const APPLICATION_VND_YOUTUBE_YT: EncodingPrefix = 2553;
    pub const APPLICATION_VND_ZUL: EncodingPrefix = 2554;
    pub const APPLICATION_VND_ZZAZZ_DECK_XML: EncodingPrefix = 2555;
    pub const APPLICATION_VOICEXML_XML: EncodingPrefix = 2556;
    pub const APPLICATION_VOUCHER_CMS_JSON: EncodingPrefix = 2557;
    pub const APPLICATION_VQ_RTCPXR: EncodingPrefix = 2558;
    pub const APPLICATION_WASM: EncodingPrefix = 2559;
    pub const APPLICATION_WATCHERINFO_XML: EncodingPrefix = 2560;
    pub const APPLICATION_WEBPUSH_OPTIONS_JSON: EncodingPrefix = 2561;
    pub const APPLICATION_WHOISPP_QUERY: EncodingPrefix = 2562;
    pub const APPLICATION_WHOISPP_RESPONSE: EncodingPrefix = 2563;
    pub const APPLICATION_WIDGET: EncodingPrefix = 2564;
    pub const APPLICATION_WITA: EncodingPrefix = 2565;
    pub const APPLICATION_WORDPERFECT5_1: EncodingPrefix = 2566;
    pub const APPLICATION_WSDL_XML: EncodingPrefix = 2567;
    pub const APPLICATION_WSPOLICY_XML: EncodingPrefix = 2568;
    pub const APPLICATION_X_PKI_MESSAGE: EncodingPrefix = 2569;
    pub const APPLICATION_X_WWW_FORM_URLENCODED: EncodingPrefix = 2570;
    pub const APPLICATION_X_X509_CA_CERT: EncodingPrefix = 2571;
    pub const APPLICATION_X_X509_CA_RA_CERT: EncodingPrefix = 2572;
    pub const APPLICATION_X_X509_NEXT_CA_CERT: EncodingPrefix = 2573;
    pub const APPLICATION_X400_BP: EncodingPrefix = 2574;
    pub const APPLICATION_XACML_XML: EncodingPrefix = 2575;
    pub const APPLICATION_XCAP_ATT_XML: EncodingPrefix = 2576;
    pub const APPLICATION_XCAP_CAPS_XML: EncodingPrefix = 2577;
    pub const APPLICATION_XCAP_DIFF_XML: EncodingPrefix = 2578;
    pub const APPLICATION_XCAP_EL_XML: EncodingPrefix = 2579;
    pub const APPLICATION_XCAP_ERROR_XML: EncodingPrefix = 2580;
    pub const APPLICATION_XCAP_NS_XML: EncodingPrefix = 2581;
    pub const APPLICATION_XCON_CONFERENCE_INFO_XML: EncodingPrefix = 2582;
    pub const APPLICATION_XCON_CONFERENCE_INFO_DIFF_XML: EncodingPrefix = 2583;
    pub const APPLICATION_XENC_XML: EncodingPrefix = 2584;
    pub const APPLICATION_XFDF: EncodingPrefix = 2585;
    pub const APPLICATION_XHTML_XML: EncodingPrefix = 2586;
    pub const APPLICATION_XLIFF_XML: EncodingPrefix = 2587;
    pub const APPLICATION_XML: EncodingPrefix = 2588;
    pub const APPLICATION_XML_DTD: EncodingPrefix = 2589;
    pub const APPLICATION_XML_EXTERNAL_PARSED_ENTITY: EncodingPrefix = 2590;
    pub const APPLICATION_XML_PATCH_XML: EncodingPrefix = 2591;
    pub const APPLICATION_XMPP_XML: EncodingPrefix = 2592;
    pub const APPLICATION_XOP_XML: EncodingPrefix = 2593;
    pub const APPLICATION_XSLT_XML: EncodingPrefix = 2594;
    pub const APPLICATION_XV_XML: EncodingPrefix = 2595;
    pub const APPLICATION_YAML: EncodingPrefix = 2596;
    pub const APPLICATION_YANG: EncodingPrefix = 2597;
    pub const APPLICATION_YANG_DATA_CBOR: EncodingPrefix = 2598;
    pub const APPLICATION_YANG_DATA_JSON: EncodingPrefix = 2599;
    pub const APPLICATION_YANG_DATA_XML: EncodingPrefix = 2600;
    pub const APPLICATION_YANG_PATCH_JSON: EncodingPrefix = 2601;
    pub const APPLICATION_YANG_PATCH_XML: EncodingPrefix = 2602;
    pub const APPLICATION_YANG_SID_JSON: EncodingPrefix = 2603;
    pub const APPLICATION_YIN_XML: EncodingPrefix = 2604;
    pub const APPLICATION_ZIP: EncodingPrefix = 2605;
    pub const APPLICATION_ZLIB: EncodingPrefix = 2606;
    pub const APPLICATION_ZSTD: EncodingPrefix = 2607;
    pub const AUDIO_1D_INTERLEAVED_PARITYFEC: EncodingPrefix = 2608;
    pub const AUDIO_32KADPCM: EncodingPrefix = 2609;
    pub const AUDIO_3GPP: EncodingPrefix = 2610;
    pub const AUDIO_3GPP2: EncodingPrefix = 2611;
    pub const AUDIO_AMR: EncodingPrefix = 2612;
    pub const AUDIO_AMR_WB: EncodingPrefix = 2613;
    pub const AUDIO_ATRAC_ADVANCED_LOSSLESS: EncodingPrefix = 2614;
    pub const AUDIO_ATRAC_X: EncodingPrefix = 2615;
    pub const AUDIO_ATRAC3: EncodingPrefix = 2616;
    pub const AUDIO_BV16: EncodingPrefix = 2617;
    pub const AUDIO_BV32: EncodingPrefix = 2618;
    pub const AUDIO_CN: EncodingPrefix = 2619;
    pub const AUDIO_DAT12: EncodingPrefix = 2620;
    pub const AUDIO_DV: EncodingPrefix = 2621;
    pub const AUDIO_DVI4: EncodingPrefix = 2622;
    pub const AUDIO_EVRC: EncodingPrefix = 2623;
    pub const AUDIO_EVRC_QCP: EncodingPrefix = 2624;
    pub const AUDIO_EVRC0: EncodingPrefix = 2625;
    pub const AUDIO_EVRC1: EncodingPrefix = 2626;
    pub const AUDIO_EVRCB: EncodingPrefix = 2627;
    pub const AUDIO_EVRCB0: EncodingPrefix = 2628;
    pub const AUDIO_EVRCB1: EncodingPrefix = 2629;
    pub const AUDIO_EVRCNW: EncodingPrefix = 2630;
    pub const AUDIO_EVRCNW0: EncodingPrefix = 2631;
    pub const AUDIO_EVRCNW1: EncodingPrefix = 2632;
    pub const AUDIO_EVRCWB: EncodingPrefix = 2633;
    pub const AUDIO_EVRCWB0: EncodingPrefix = 2634;
    pub const AUDIO_EVRCWB1: EncodingPrefix = 2635;
    pub const AUDIO_EVS: EncodingPrefix = 2636;
    pub const AUDIO_G711_0: EncodingPrefix = 2637;
    pub const AUDIO_G719: EncodingPrefix = 2638;
    pub const AUDIO_G722: EncodingPrefix = 2639;
    pub const AUDIO_G7221: EncodingPrefix = 2640;
    pub const AUDIO_G723: EncodingPrefix = 2641;
    pub const AUDIO_G726_16: EncodingPrefix = 2642;
    pub const AUDIO_G726_24: EncodingPrefix = 2643;
    pub const AUDIO_G726_32: EncodingPrefix = 2644;
    pub const AUDIO_G726_40: EncodingPrefix = 2645;
    pub const AUDIO_G728: EncodingPrefix = 2646;
    pub const AUDIO_G729: EncodingPrefix = 2647;
    pub const AUDIO_G7291: EncodingPrefix = 2648;
    pub const AUDIO_G729D: EncodingPrefix = 2649;
    pub const AUDIO_G729E: EncodingPrefix = 2650;
    pub const AUDIO_GSM: EncodingPrefix = 2651;
    pub const AUDIO_GSM_EFR: EncodingPrefix = 2652;
    pub const AUDIO_GSM_HR_08: EncodingPrefix = 2653;
    pub const AUDIO_L16: EncodingPrefix = 2654;
    pub const AUDIO_L20: EncodingPrefix = 2655;
    pub const AUDIO_L24: EncodingPrefix = 2656;
    pub const AUDIO_L8: EncodingPrefix = 2657;
    pub const AUDIO_LPC: EncodingPrefix = 2658;
    pub const AUDIO_MELP: EncodingPrefix = 2659;
    pub const AUDIO_MELP1200: EncodingPrefix = 2660;
    pub const AUDIO_MELP2400: EncodingPrefix = 2661;
    pub const AUDIO_MELP600: EncodingPrefix = 2662;
    pub const AUDIO_MP4A_LATM: EncodingPrefix = 2663;
    pub const AUDIO_MPA: EncodingPrefix = 2664;
    pub const AUDIO_PCMA: EncodingPrefix = 2665;
    pub const AUDIO_PCMA_WB: EncodingPrefix = 2666;
    pub const AUDIO_PCMU: EncodingPrefix = 2667;
    pub const AUDIO_PCMU_WB: EncodingPrefix = 2668;
    pub const AUDIO_QCELP: EncodingPrefix = 2669;
    pub const AUDIO_RED: EncodingPrefix = 2670;
    pub const AUDIO_SMV: EncodingPrefix = 2671;
    pub const AUDIO_SMV_QCP: EncodingPrefix = 2672;
    pub const AUDIO_SMV0: EncodingPrefix = 2673;
    pub const AUDIO_TETRA_ACELP: EncodingPrefix = 2674;
    pub const AUDIO_TETRA_ACELP_BB: EncodingPrefix = 2675;
    pub const AUDIO_TSVCIS: EncodingPrefix = 2676;
    pub const AUDIO_UEMCLIP: EncodingPrefix = 2677;
    pub const AUDIO_VDVI: EncodingPrefix = 2678;
    pub const AUDIO_VMR_WB: EncodingPrefix = 2679;
    pub const AUDIO_AAC: EncodingPrefix = 2680;
    pub const AUDIO_AC3: EncodingPrefix = 2681;
    pub const AUDIO_AMR_WB_P: EncodingPrefix = 2682;
    pub const AUDIO_APTX: EncodingPrefix = 2683;
    pub const AUDIO_ASC: EncodingPrefix = 2684;
    pub const AUDIO_BASIC: EncodingPrefix = 2685;
    pub const AUDIO_CLEARMODE: EncodingPrefix = 2686;
    pub const AUDIO_DLS: EncodingPrefix = 2687;
    pub const AUDIO_DSR_ES201108: EncodingPrefix = 2688;
    pub const AUDIO_DSR_ES202050: EncodingPrefix = 2689;
    pub const AUDIO_DSR_ES202211: EncodingPrefix = 2690;
    pub const AUDIO_DSR_ES202212: EncodingPrefix = 2691;
    pub const AUDIO_EAC3: EncodingPrefix = 2692;
    pub const AUDIO_ENCAPRTP: EncodingPrefix = 2693;
    pub const AUDIO_EXAMPLE: EncodingPrefix = 2694;
    pub const AUDIO_FLEXFEC: EncodingPrefix = 2695;
    pub const AUDIO_FWDRED: EncodingPrefix = 2696;
    pub const AUDIO_ILBC: EncodingPrefix = 2697;
    pub const AUDIO_IP_MR_V2_5: EncodingPrefix = 2698;
    pub const AUDIO_MATROSKA: EncodingPrefix = 2699;
    pub const AUDIO_MHAS: EncodingPrefix = 2700;
    pub const AUDIO_MOBILE_XMF: EncodingPrefix = 2701;
    pub const AUDIO_MP4: EncodingPrefix = 2702;
    pub const AUDIO_MPA_ROBUST: EncodingPrefix = 2703;
    pub const AUDIO_MPEG: EncodingPrefix = 2704;
    pub const AUDIO_MPEG4_GENERIC: EncodingPrefix = 2705;
    pub const AUDIO_OGG: EncodingPrefix = 2706;
    pub const AUDIO_OPUS: EncodingPrefix = 2707;
    pub const AUDIO_PARITYFEC: EncodingPrefix = 2708;
    pub const AUDIO_PRS_SID: EncodingPrefix = 2709;
    pub const AUDIO_RAPTORFEC: EncodingPrefix = 2710;
    pub const AUDIO_RTP_ENC_AESCM128: EncodingPrefix = 2711;
    pub const AUDIO_RTP_MIDI: EncodingPrefix = 2712;
    pub const AUDIO_RTPLOOPBACK: EncodingPrefix = 2713;
    pub const AUDIO_RTX: EncodingPrefix = 2714;
    pub const AUDIO_SCIP: EncodingPrefix = 2715;
    pub const AUDIO_SOFA: EncodingPrefix = 2716;
    pub const AUDIO_SP_MIDI: EncodingPrefix = 2717;
    pub const AUDIO_SPEEX: EncodingPrefix = 2718;
    pub const AUDIO_T140C: EncodingPrefix = 2719;
    pub const AUDIO_T38: EncodingPrefix = 2720;
    pub const AUDIO_TELEPHONE_EVENT: EncodingPrefix = 2721;
    pub const AUDIO_TONE: EncodingPrefix = 2722;
    pub const AUDIO_ULPFEC: EncodingPrefix = 2723;
    pub const AUDIO_USAC: EncodingPrefix = 2724;
    pub const AUDIO_VND_3GPP_IUFP: EncodingPrefix = 2725;
    pub const AUDIO_VND_4SB: EncodingPrefix = 2726;
    pub const AUDIO_VND_CELP: EncodingPrefix = 2727;
    pub const AUDIO_VND_AUDIOKOZ: EncodingPrefix = 2728;
    pub const AUDIO_VND_CISCO_NSE: EncodingPrefix = 2729;
    pub const AUDIO_VND_CMLES_RADIO_EVENTS: EncodingPrefix = 2730;
    pub const AUDIO_VND_CNS_ANP1: EncodingPrefix = 2731;
    pub const AUDIO_VND_CNS_INF1: EncodingPrefix = 2732;
    pub const AUDIO_VND_DECE_AUDIO: EncodingPrefix = 2733;
    pub const AUDIO_VND_DIGITAL_WINDS: EncodingPrefix = 2734;
    pub const AUDIO_VND_DLNA_ADTS: EncodingPrefix = 2735;
    pub const AUDIO_VND_DOLBY_HEAAC_1: EncodingPrefix = 2736;
    pub const AUDIO_VND_DOLBY_HEAAC_2: EncodingPrefix = 2737;
    pub const AUDIO_VND_DOLBY_MLP: EncodingPrefix = 2738;
    pub const AUDIO_VND_DOLBY_MPS: EncodingPrefix = 2739;
    pub const AUDIO_VND_DOLBY_PL2: EncodingPrefix = 2740;
    pub const AUDIO_VND_DOLBY_PL2X: EncodingPrefix = 2741;
    pub const AUDIO_VND_DOLBY_PL2Z: EncodingPrefix = 2742;
    pub const AUDIO_VND_DOLBY_PULSE_1: EncodingPrefix = 2743;
    pub const AUDIO_VND_DRA: EncodingPrefix = 2744;
    pub const AUDIO_VND_DTS: EncodingPrefix = 2745;
    pub const AUDIO_VND_DTS_HD: EncodingPrefix = 2746;
    pub const AUDIO_VND_DTS_UHD: EncodingPrefix = 2747;
    pub const AUDIO_VND_DVB_FILE: EncodingPrefix = 2748;
    pub const AUDIO_VND_EVERAD_PLJ: EncodingPrefix = 2749;
    pub const AUDIO_VND_HNS_AUDIO: EncodingPrefix = 2750;
    pub const AUDIO_VND_LUCENT_VOICE: EncodingPrefix = 2751;
    pub const AUDIO_VND_MS_PLAYREADY_MEDIA_PYA: EncodingPrefix = 2752;
    pub const AUDIO_VND_NOKIA_MOBILE_XMF: EncodingPrefix = 2753;
    pub const AUDIO_VND_NORTEL_VBK: EncodingPrefix = 2754;
    pub const AUDIO_VND_NUERA_ECELP4800: EncodingPrefix = 2755;
    pub const AUDIO_VND_NUERA_ECELP7470: EncodingPrefix = 2756;
    pub const AUDIO_VND_NUERA_ECELP9600: EncodingPrefix = 2757;
    pub const AUDIO_VND_OCTEL_SBC: EncodingPrefix = 2758;
    pub const AUDIO_VND_PRESONUS_MULTITRACK: EncodingPrefix = 2759;
    pub const AUDIO_VND_QCELP: EncodingPrefix = 2760;
    pub const AUDIO_VND_RHETOREX_32KADPCM: EncodingPrefix = 2761;
    pub const AUDIO_VND_RIP: EncodingPrefix = 2762;
    pub const AUDIO_VND_SEALEDMEDIA_SOFTSEAL_MPEG: EncodingPrefix = 2763;
    pub const AUDIO_VND_VMX_CVSD: EncodingPrefix = 2764;
    pub const AUDIO_VORBIS: EncodingPrefix = 2765;
    pub const AUDIO_VORBIS_CONFIG: EncodingPrefix = 2766;
    pub const FONT_COLLECTION: EncodingPrefix = 2767;
    pub const FONT_OTF: EncodingPrefix = 2768;
    pub const FONT_SFNT: EncodingPrefix = 2769;
    pub const FONT_TTF: EncodingPrefix = 2770;
    pub const FONT_WOFF: EncodingPrefix = 2771;
    pub const FONT_WOFF2: EncodingPrefix = 2772;
    pub const IMAGE_ACES: EncodingPrefix = 2773;
    pub const IMAGE_APNG: EncodingPrefix = 2774;
    pub const IMAGE_AVCI: EncodingPrefix = 2775;
    pub const IMAGE_AVCS: EncodingPrefix = 2776;
    pub const IMAGE_AVIF: EncodingPrefix = 2777;
    pub const IMAGE_BMP: EncodingPrefix = 2778;
    pub const IMAGE_CGM: EncodingPrefix = 2779;
    pub const IMAGE_DICOM_RLE: EncodingPrefix = 2780;
    pub const IMAGE_DPX: EncodingPrefix = 2781;
    pub const IMAGE_EMF: EncodingPrefix = 2782;
    pub const IMAGE_EXAMPLE: EncodingPrefix = 2783;
    pub const IMAGE_FITS: EncodingPrefix = 2784;
    pub const IMAGE_G3FAX: EncodingPrefix = 2785;
    pub const IMAGE_GIF: EncodingPrefix = 2786;
    pub const IMAGE_HEIC: EncodingPrefix = 2787;
    pub const IMAGE_HEIC_SEQUENCE: EncodingPrefix = 2788;
    pub const IMAGE_HEIF: EncodingPrefix = 2789;
    pub const IMAGE_HEIF_SEQUENCE: EncodingPrefix = 2790;
    pub const IMAGE_HEJ2K: EncodingPrefix = 2791;
    pub const IMAGE_HSJ2: EncodingPrefix = 2792;
    pub const IMAGE_IEF: EncodingPrefix = 2793;
    pub const IMAGE_J2C: EncodingPrefix = 2794;
    pub const IMAGE_JLS: EncodingPrefix = 2795;
    pub const IMAGE_JP2: EncodingPrefix = 2796;
    pub const IMAGE_JPEG: EncodingPrefix = 2797;
    pub const IMAGE_JPH: EncodingPrefix = 2798;
    pub const IMAGE_JPHC: EncodingPrefix = 2799;
    pub const IMAGE_JPM: EncodingPrefix = 2800;
    pub const IMAGE_JPX: EncodingPrefix = 2801;
    pub const IMAGE_JXR: EncodingPrefix = 2802;
    pub const IMAGE_JXRA: EncodingPrefix = 2803;
    pub const IMAGE_JXRS: EncodingPrefix = 2804;
    pub const IMAGE_JXS: EncodingPrefix = 2805;
    pub const IMAGE_JXSC: EncodingPrefix = 2806;
    pub const IMAGE_JXSI: EncodingPrefix = 2807;
    pub const IMAGE_JXSS: EncodingPrefix = 2808;
    pub const IMAGE_KTX: EncodingPrefix = 2809;
    pub const IMAGE_KTX2: EncodingPrefix = 2810;
    pub const IMAGE_NAPLPS: EncodingPrefix = 2811;
    pub const IMAGE_PNG: EncodingPrefix = 2812;
    pub const IMAGE_PRS_BTIF: EncodingPrefix = 2813;
    pub const IMAGE_PRS_PTI: EncodingPrefix = 2814;
    pub const IMAGE_PWG_RASTER: EncodingPrefix = 2815;
    pub const IMAGE_SVG_XML: EncodingPrefix = 2816;
    pub const IMAGE_T38: EncodingPrefix = 2817;
    pub const IMAGE_TIFF: EncodingPrefix = 2818;
    pub const IMAGE_TIFF_FX: EncodingPrefix = 2819;
    pub const IMAGE_VND_ADOBE_PHOTOSHOP: EncodingPrefix = 2820;
    pub const IMAGE_VND_AIRZIP_ACCELERATOR_AZV: EncodingPrefix = 2821;
    pub const IMAGE_VND_CNS_INF2: EncodingPrefix = 2822;
    pub const IMAGE_VND_DECE_GRAPHIC: EncodingPrefix = 2823;
    pub const IMAGE_VND_DJVU: EncodingPrefix = 2824;
    pub const IMAGE_VND_DVB_SUBTITLE: EncodingPrefix = 2825;
    pub const IMAGE_VND_DWG: EncodingPrefix = 2826;
    pub const IMAGE_VND_DXF: EncodingPrefix = 2827;
    pub const IMAGE_VND_FASTBIDSHEET: EncodingPrefix = 2828;
    pub const IMAGE_VND_FPX: EncodingPrefix = 2829;
    pub const IMAGE_VND_FST: EncodingPrefix = 2830;
    pub const IMAGE_VND_FUJIXEROX_EDMICS_MMR: EncodingPrefix = 2831;
    pub const IMAGE_VND_FUJIXEROX_EDMICS_RLC: EncodingPrefix = 2832;
    pub const IMAGE_VND_GLOBALGRAPHICS_PGB: EncodingPrefix = 2833;
    pub const IMAGE_VND_MICROSOFT_ICON: EncodingPrefix = 2834;
    pub const IMAGE_VND_MIX: EncodingPrefix = 2835;
    pub const IMAGE_VND_MOZILLA_APNG: EncodingPrefix = 2836;
    pub const IMAGE_VND_MS_MODI: EncodingPrefix = 2837;
    pub const IMAGE_VND_NET_FPX: EncodingPrefix = 2838;
    pub const IMAGE_VND_PCO_B16: EncodingPrefix = 2839;
    pub const IMAGE_VND_RADIANCE: EncodingPrefix = 2840;
    pub const IMAGE_VND_SEALED_PNG: EncodingPrefix = 2841;
    pub const IMAGE_VND_SEALEDMEDIA_SOFTSEAL_GIF: EncodingPrefix = 2842;
    pub const IMAGE_VND_SEALEDMEDIA_SOFTSEAL_JPG: EncodingPrefix = 2843;
    pub const IMAGE_VND_SVF: EncodingPrefix = 2844;
    pub const IMAGE_VND_TENCENT_TAP: EncodingPrefix = 2845;
    pub const IMAGE_VND_VALVE_SOURCE_TEXTURE: EncodingPrefix = 2846;
    pub const IMAGE_VND_WAP_WBMP: EncodingPrefix = 2847;
    pub const IMAGE_VND_XIFF: EncodingPrefix = 2848;
    pub const IMAGE_VND_ZBRUSH_PCX: EncodingPrefix = 2849;
    pub const IMAGE_WEBP: EncodingPrefix = 2850;
    pub const IMAGE_WMF: EncodingPrefix = 2851;
    pub const MESSAGE_CPIM: EncodingPrefix = 2852;
    pub const MESSAGE_BHTTP: EncodingPrefix = 2853;
    pub const MESSAGE_DELIVERY_STATUS: EncodingPrefix = 2854;
    pub const MESSAGE_DISPOSITION_NOTIFICATION: EncodingPrefix = 2855;
    pub const MESSAGE_EXAMPLE: EncodingPrefix = 2856;
    pub const MESSAGE_EXTERNAL_BODY: EncodingPrefix = 2857;
    pub const MESSAGE_FEEDBACK_REPORT: EncodingPrefix = 2858;
    pub const MESSAGE_GLOBAL: EncodingPrefix = 2859;
    pub const MESSAGE_GLOBAL_DELIVERY_STATUS: EncodingPrefix = 2860;
    pub const MESSAGE_GLOBAL_DISPOSITION_NOTIFICATION: EncodingPrefix = 2861;
    pub const MESSAGE_GLOBAL_HEADERS: EncodingPrefix = 2862;
    pub const MESSAGE_HTTP: EncodingPrefix = 2863;
    pub const MESSAGE_IMDN_XML: EncodingPrefix = 2864;
    pub const MESSAGE_MLS: EncodingPrefix = 2865;
    pub const MESSAGE_NEWS: EncodingPrefix = 2866;
    pub const MESSAGE_OHTTP_REQ: EncodingPrefix = 2867;
    pub const MESSAGE_OHTTP_RES: EncodingPrefix = 2868;
    pub const MESSAGE_PARTIAL: EncodingPrefix = 2869;
    pub const MESSAGE_RFC822: EncodingPrefix = 2870;
    pub const MESSAGE_S_HTTP: EncodingPrefix = 2871;
    pub const MESSAGE_SIP: EncodingPrefix = 2872;
    pub const MESSAGE_SIPFRAG: EncodingPrefix = 2873;
    pub const MESSAGE_TRACKING_STATUS: EncodingPrefix = 2874;
    pub const MESSAGE_VND_SI_SIMP: EncodingPrefix = 2875;
    pub const MESSAGE_VND_WFA_WSC: EncodingPrefix = 2876;
    pub const MODEL_3MF: EncodingPrefix = 2877;
    pub const MODEL_JT: EncodingPrefix = 2878;
    pub const MODEL_E57: EncodingPrefix = 2879;
    pub const MODEL_EXAMPLE: EncodingPrefix = 2880;
    pub const MODEL_GLTF_JSON: EncodingPrefix = 2881;
    pub const MODEL_GLTF_BINARY: EncodingPrefix = 2882;
    pub const MODEL_IGES: EncodingPrefix = 2883;
    pub const MODEL_MESH: EncodingPrefix = 2884;
    pub const MODEL_MTL: EncodingPrefix = 2885;
    pub const MODEL_OBJ: EncodingPrefix = 2886;
    pub const MODEL_PRC: EncodingPrefix = 2887;
    pub const MODEL_STEP: EncodingPrefix = 2888;
    pub const MODEL_STEP_XML: EncodingPrefix = 2889;
    pub const MODEL_STEP_ZIP: EncodingPrefix = 2890;
    pub const MODEL_STEP_XML_ZIP: EncodingPrefix = 2891;
    pub const MODEL_STL: EncodingPrefix = 2892;
    pub const MODEL_U3D: EncodingPrefix = 2893;
    pub const MODEL_VND_BARY: EncodingPrefix = 2894;
    pub const MODEL_VND_CLD: EncodingPrefix = 2895;
    pub const MODEL_VND_COLLADA_XML: EncodingPrefix = 2896;
    pub const MODEL_VND_DWF: EncodingPrefix = 2897;
    pub const MODEL_VND_FLATLAND_3DML: EncodingPrefix = 2898;
    pub const MODEL_VND_GDL: EncodingPrefix = 2899;
    pub const MODEL_VND_GS_GDL: EncodingPrefix = 2900;
    pub const MODEL_VND_GTW: EncodingPrefix = 2901;
    pub const MODEL_VND_MOML_XML: EncodingPrefix = 2902;
    pub const MODEL_VND_MTS: EncodingPrefix = 2903;
    pub const MODEL_VND_OPENGEX: EncodingPrefix = 2904;
    pub const MODEL_VND_PARASOLID_TRANSMIT_BINARY: EncodingPrefix = 2905;
    pub const MODEL_VND_PARASOLID_TRANSMIT_TEXT: EncodingPrefix = 2906;
    pub const MODEL_VND_PYTHA_PYOX: EncodingPrefix = 2907;
    pub const MODEL_VND_ROSETTE_ANNOTATED_DATA_MODEL: EncodingPrefix = 2908;
    pub const MODEL_VND_SAP_VDS: EncodingPrefix = 2909;
    pub const MODEL_VND_USDA: EncodingPrefix = 2910;
    pub const MODEL_VND_USDZ_ZIP: EncodingPrefix = 2911;
    pub const MODEL_VND_VALVE_SOURCE_COMPILED_MAP: EncodingPrefix = 2912;
    pub const MODEL_VND_VTU: EncodingPrefix = 2913;
    pub const MODEL_VRML: EncodingPrefix = 2914;
    pub const MODEL_X3D_FASTINFOSET: EncodingPrefix = 2915;
    pub const MODEL_X3D_XML: EncodingPrefix = 2916;
    pub const MODEL_X3D_VRML: EncodingPrefix = 2917;
    pub const MULTIPART_ALTERNATIVE: EncodingPrefix = 2918;
    pub const MULTIPART_APPLEDOUBLE: EncodingPrefix = 2919;
    pub const MULTIPART_BYTERANGES: EncodingPrefix = 2920;
    pub const MULTIPART_DIGEST: EncodingPrefix = 2921;
    pub const MULTIPART_ENCRYPTED: EncodingPrefix = 2922;
    pub const MULTIPART_EXAMPLE: EncodingPrefix = 2923;
    pub const MULTIPART_FORM_DATA: EncodingPrefix = 2924;
    pub const MULTIPART_HEADER_SET: EncodingPrefix = 2925;
    pub const MULTIPART_MIXED: EncodingPrefix = 2926;
    pub const MULTIPART_MULTILINGUAL: EncodingPrefix = 2927;
    pub const MULTIPART_PARALLEL: EncodingPrefix = 2928;
    pub const MULTIPART_RELATED: EncodingPrefix = 2929;
    pub const MULTIPART_REPORT: EncodingPrefix = 2930;
    pub const MULTIPART_SIGNED: EncodingPrefix = 2931;
    pub const MULTIPART_VND_BINT_MED_PLUS: EncodingPrefix = 2932;
    pub const MULTIPART_VOICE_MESSAGE: EncodingPrefix = 2933;
    pub const MULTIPART_X_MIXED_REPLACE: EncodingPrefix = 2934;
    pub const TEXT_1D_INTERLEAVED_PARITYFEC: EncodingPrefix = 2935;
    pub const TEXT_RED: EncodingPrefix = 2936;
    pub const TEXT_SGML: EncodingPrefix = 2937;
    pub const TEXT_CACHE_MANIFEST: EncodingPrefix = 2938;
    pub const TEXT_CALENDAR: EncodingPrefix = 2939;
    pub const TEXT_CQL: EncodingPrefix = 2940;
    pub const TEXT_CQL_EXPRESSION: EncodingPrefix = 2941;
    pub const TEXT_CQL_IDENTIFIER: EncodingPrefix = 2942;
    pub const TEXT_CSS: EncodingPrefix = 2943;
    pub const TEXT_CSV: EncodingPrefix = 2944;
    pub const TEXT_CSV_SCHEMA: EncodingPrefix = 2945;
    pub const TEXT_DIRECTORY: EncodingPrefix = 2946;
    pub const TEXT_DNS: EncodingPrefix = 2947;
    pub const TEXT_ECMASCRIPT: EncodingPrefix = 2948;
    pub const TEXT_ENCAPRTP: EncodingPrefix = 2949;
    pub const TEXT_ENRICHED: EncodingPrefix = 2950;
    pub const TEXT_EXAMPLE: EncodingPrefix = 2951;
    pub const TEXT_FHIRPATH: EncodingPrefix = 2952;
    pub const TEXT_FLEXFEC: EncodingPrefix = 2953;
    pub const TEXT_FWDRED: EncodingPrefix = 2954;
    pub const TEXT_GFF3: EncodingPrefix = 2955;
    pub const TEXT_GRAMMAR_REF_LIST: EncodingPrefix = 2956;
    pub const TEXT_HL7V2: EncodingPrefix = 2957;
    pub const TEXT_HTML: EncodingPrefix = 2958;
    pub const TEXT_JAVASCRIPT: EncodingPrefix = 2959;
    pub const TEXT_JCR_CND: EncodingPrefix = 2960;
    pub const TEXT_MARKDOWN: EncodingPrefix = 2961;
    pub const TEXT_MIZAR: EncodingPrefix = 2962;
    pub const TEXT_N3: EncodingPrefix = 2963;
    pub const TEXT_PARAMETERS: EncodingPrefix = 2964;
    pub const TEXT_PARITYFEC: EncodingPrefix = 2965;
    pub const TEXT_PLAIN: EncodingPrefix = 2966;
    pub const TEXT_PROVENANCE_NOTATION: EncodingPrefix = 2967;
    pub const TEXT_PRS_FALLENSTEIN_RST: EncodingPrefix = 2968;
    pub const TEXT_PRS_LINES_TAG: EncodingPrefix = 2969;
    pub const TEXT_PRS_PROP_LOGIC: EncodingPrefix = 2970;
    pub const TEXT_PRS_TEXI: EncodingPrefix = 2971;
    pub const TEXT_RAPTORFEC: EncodingPrefix = 2972;
    pub const TEXT_RFC822_HEADERS: EncodingPrefix = 2973;
    pub const TEXT_RICHTEXT: EncodingPrefix = 2974;
    pub const TEXT_RTF: EncodingPrefix = 2975;
    pub const TEXT_RTP_ENC_AESCM128: EncodingPrefix = 2976;
    pub const TEXT_RTPLOOPBACK: EncodingPrefix = 2977;
    pub const TEXT_RTX: EncodingPrefix = 2978;
    pub const TEXT_SHACLC: EncodingPrefix = 2979;
    pub const TEXT_SHEX: EncodingPrefix = 2980;
    pub const TEXT_SPDX: EncodingPrefix = 2981;
    pub const TEXT_STRINGS: EncodingPrefix = 2982;
    pub const TEXT_T140: EncodingPrefix = 2983;
    pub const TEXT_TAB_SEPARATED_VALUES: EncodingPrefix = 2984;
    pub const TEXT_TROFF: EncodingPrefix = 2985;
    pub const TEXT_TURTLE: EncodingPrefix = 2986;
    pub const TEXT_ULPFEC: EncodingPrefix = 2987;
    pub const TEXT_URI_LIST: EncodingPrefix = 2988;
    pub const TEXT_VCARD: EncodingPrefix = 2989;
    pub const TEXT_VND_DMCLIENTSCRIPT: EncodingPrefix = 2990;
    pub const TEXT_VND_IPTC_NITF: EncodingPrefix = 2991;
    pub const TEXT_VND_IPTC_NEWSML: EncodingPrefix = 2992;
    pub const TEXT_VND_A: EncodingPrefix = 2993;
    pub const TEXT_VND_ABC: EncodingPrefix = 2994;
    pub const TEXT_VND_ASCII_ART: EncodingPrefix = 2995;
    pub const TEXT_VND_CURL: EncodingPrefix = 2996;
    pub const TEXT_VND_DEBIAN_COPYRIGHT: EncodingPrefix = 2997;
    pub const TEXT_VND_DVB_SUBTITLE: EncodingPrefix = 2998;
    pub const TEXT_VND_ESMERTEC_THEME_DESCRIPTOR: EncodingPrefix = 2999;
    pub const TEXT_VND_EXCHANGEABLE: EncodingPrefix = 3000;
    pub const TEXT_VND_FAMILYSEARCH_GEDCOM: EncodingPrefix = 3001;
    pub const TEXT_VND_FICLAB_FLT: EncodingPrefix = 3002;
    pub const TEXT_VND_FLY: EncodingPrefix = 3003;
    pub const TEXT_VND_FMI_FLEXSTOR: EncodingPrefix = 3004;
    pub const TEXT_VND_GML: EncodingPrefix = 3005;
    pub const TEXT_VND_GRAPHVIZ: EncodingPrefix = 3006;
    pub const TEXT_VND_HANS: EncodingPrefix = 3007;
    pub const TEXT_VND_HGL: EncodingPrefix = 3008;
    pub const TEXT_VND_IN3D_3DML: EncodingPrefix = 3009;
    pub const TEXT_VND_IN3D_SPOT: EncodingPrefix = 3010;
    pub const TEXT_VND_LATEX_Z: EncodingPrefix = 3011;
    pub const TEXT_VND_MOTOROLA_REFLEX: EncodingPrefix = 3012;
    pub const TEXT_VND_MS_MEDIAPACKAGE: EncodingPrefix = 3013;
    pub const TEXT_VND_NET2PHONE_COMMCENTER_COMMAND: EncodingPrefix = 3014;
    pub const TEXT_VND_RADISYS_MSML_BASIC_LAYOUT: EncodingPrefix = 3015;
    pub const TEXT_VND_SENX_WARPSCRIPT: EncodingPrefix = 3016;
    pub const TEXT_VND_SI_URICATALOGUE: EncodingPrefix = 3017;
    pub const TEXT_VND_SOSI: EncodingPrefix = 3018;
    pub const TEXT_VND_SUN_J2ME_APP_DESCRIPTOR: EncodingPrefix = 3019;
    pub const TEXT_VND_TROLLTECH_LINGUIST: EncodingPrefix = 3020;
    pub const TEXT_VND_WAP_SI: EncodingPrefix = 3021;
    pub const TEXT_VND_WAP_SL: EncodingPrefix = 3022;
    pub const TEXT_VND_WAP_WML: EncodingPrefix = 3023;
    pub const TEXT_VND_WAP_WMLSCRIPT: EncodingPrefix = 3024;
    pub const TEXT_VTT: EncodingPrefix = 3025;
    pub const TEXT_WGSL: EncodingPrefix = 3026;
    pub const TEXT_XML: EncodingPrefix = 3027;
    pub const TEXT_XML_EXTERNAL_PARSED_ENTITY: EncodingPrefix = 3028;
    pub const VIDEO_1D_INTERLEAVED_PARITYFEC: EncodingPrefix = 3029;
    pub const VIDEO_3GPP: EncodingPrefix = 3030;
    pub const VIDEO_3GPP_TT: EncodingPrefix = 3031;
    pub const VIDEO_3GPP2: EncodingPrefix = 3032;
    pub const VIDEO_AV1: EncodingPrefix = 3033;
    pub const VIDEO_BMPEG: EncodingPrefix = 3034;
    pub const VIDEO_BT656: EncodingPrefix = 3035;
    pub const VIDEO_CELB: EncodingPrefix = 3036;
    pub const VIDEO_DV: EncodingPrefix = 3037;
    pub const VIDEO_FFV1: EncodingPrefix = 3038;
    pub const VIDEO_H261: EncodingPrefix = 3039;
    pub const VIDEO_H263: EncodingPrefix = 3040;
    pub const VIDEO_H263_1998: EncodingPrefix = 3041;
    pub const VIDEO_H263_2000: EncodingPrefix = 3042;
    pub const VIDEO_H264: EncodingPrefix = 3043;
    pub const VIDEO_H264_RCDO: EncodingPrefix = 3044;
    pub const VIDEO_H264_SVC: EncodingPrefix = 3045;
    pub const VIDEO_H265: EncodingPrefix = 3046;
    pub const VIDEO_H266: EncodingPrefix = 3047;
    pub const VIDEO_JPEG: EncodingPrefix = 3048;
    pub const VIDEO_MP1S: EncodingPrefix = 3049;
    pub const VIDEO_MP2P: EncodingPrefix = 3050;
    pub const VIDEO_MP2T: EncodingPrefix = 3051;
    pub const VIDEO_MP4V_ES: EncodingPrefix = 3052;
    pub const VIDEO_MPV: EncodingPrefix = 3053;
    pub const VIDEO_SMPTE292M: EncodingPrefix = 3054;
    pub const VIDEO_VP8: EncodingPrefix = 3055;
    pub const VIDEO_VP9: EncodingPrefix = 3056;
    pub const VIDEO_ENCAPRTP: EncodingPrefix = 3057;
    pub const VIDEO_EVC: EncodingPrefix = 3058;
    pub const VIDEO_EXAMPLE: EncodingPrefix = 3059;
    pub const VIDEO_FLEXFEC: EncodingPrefix = 3060;
    pub const VIDEO_ISO_SEGMENT: EncodingPrefix = 3061;
    pub const VIDEO_JPEG2000: EncodingPrefix = 3062;
    pub const VIDEO_JXSV: EncodingPrefix = 3063;
    pub const VIDEO_MATROSKA: EncodingPrefix = 3064;
    pub const VIDEO_MATROSKA_3D: EncodingPrefix = 3065;
    pub const VIDEO_MJ2: EncodingPrefix = 3066;
    pub const VIDEO_MP4: EncodingPrefix = 3067;
    pub const VIDEO_MPEG: EncodingPrefix = 3068;
    pub const VIDEO_MPEG4_GENERIC: EncodingPrefix = 3069;
    pub const VIDEO_NV: EncodingPrefix = 3070;
    pub const VIDEO_OGG: EncodingPrefix = 3071;
    pub const VIDEO_PARITYFEC: EncodingPrefix = 3072;
    pub const VIDEO_POINTER: EncodingPrefix = 3073;
    pub const VIDEO_QUICKTIME: EncodingPrefix = 3074;
    pub const VIDEO_RAPTORFEC: EncodingPrefix = 3075;
    pub const VIDEO_RAW: EncodingPrefix = 3076;
    pub const VIDEO_RTP_ENC_AESCM128: EncodingPrefix = 3077;
    pub const VIDEO_RTPLOOPBACK: EncodingPrefix = 3078;
    pub const VIDEO_RTX: EncodingPrefix = 3079;
    pub const VIDEO_SCIP: EncodingPrefix = 3080;
    pub const VIDEO_SMPTE291: EncodingPrefix = 3081;
    pub const VIDEO_ULPFEC: EncodingPrefix = 3082;
    pub const VIDEO_VC1: EncodingPrefix = 3083;
    pub const VIDEO_VC2: EncodingPrefix = 3084;
    pub const VIDEO_VND_CCTV: EncodingPrefix = 3085;
    pub const VIDEO_VND_DECE_HD: EncodingPrefix = 3086;
    pub const VIDEO_VND_DECE_MOBILE: EncodingPrefix = 3087;
    pub const VIDEO_VND_DECE_MP4: EncodingPrefix = 3088;
    pub const VIDEO_VND_DECE_PD: EncodingPrefix = 3089;
    pub const VIDEO_VND_DECE_SD: EncodingPrefix = 3090;
    pub const VIDEO_VND_DECE_VIDEO: EncodingPrefix = 3091;
    pub const VIDEO_VND_DIRECTV_MPEG: EncodingPrefix = 3092;
    pub const VIDEO_VND_DIRECTV_MPEG_TTS: EncodingPrefix = 3093;
    pub const VIDEO_VND_DLNA_MPEG_TTS: EncodingPrefix = 3094;
    pub const VIDEO_VND_DVB_FILE: EncodingPrefix = 3095;
    pub const VIDEO_VND_FVT: EncodingPrefix = 3096;
    pub const VIDEO_VND_HNS_VIDEO: EncodingPrefix = 3097;
    pub const VIDEO_VND_IPTVFORUM_1DPARITYFEC_1010: EncodingPrefix = 3098;
    pub const VIDEO_VND_IPTVFORUM_1DPARITYFEC_2005: EncodingPrefix = 3099;
    pub const VIDEO_VND_IPTVFORUM_2DPARITYFEC_1010: EncodingPrefix = 3100;
    pub const VIDEO_VND_IPTVFORUM_2DPARITYFEC_2005: EncodingPrefix = 3101;
    pub const VIDEO_VND_IPTVFORUM_TTSAVC: EncodingPrefix = 3102;
    pub const VIDEO_VND_IPTVFORUM_TTSMPEG2: EncodingPrefix = 3103;
    pub const VIDEO_VND_MOTOROLA_VIDEO: EncodingPrefix = 3104;
    pub const VIDEO_VND_MOTOROLA_VIDEOP: EncodingPrefix = 3105;
    pub const VIDEO_VND_MPEGURL: EncodingPrefix = 3106;
    pub const VIDEO_VND_MS_PLAYREADY_MEDIA_PYV: EncodingPrefix = 3107;
    pub const VIDEO_VND_NOKIA_INTERLEAVED_MULTIMEDIA: EncodingPrefix = 3108;
    pub const VIDEO_VND_NOKIA_MP4VR: EncodingPrefix = 3109;
    pub const VIDEO_VND_NOKIA_VIDEOVOIP: EncodingPrefix = 3110;
    pub const VIDEO_VND_OBJECTVIDEO: EncodingPrefix = 3111;
    pub const VIDEO_VND_RADGAMETTOOLS_BINK: EncodingPrefix = 3112;
    pub const VIDEO_VND_RADGAMETTOOLS_SMACKER: EncodingPrefix = 3113;
    pub const VIDEO_VND_SEALED_MPEG1: EncodingPrefix = 3114;
    pub const VIDEO_VND_SEALED_MPEG4: EncodingPrefix = 3115;
    pub const VIDEO_VND_SEALED_SWF: EncodingPrefix = 3116;
    pub const VIDEO_VND_SEALEDMEDIA_SOFTSEAL_MOV: EncodingPrefix = 3117;
    pub const VIDEO_VND_UVVU_MP4: EncodingPrefix = 3118;
    pub const VIDEO_VND_VIVO: EncodingPrefix = 3119;
    pub const VIDEO_VND_YOUTUBE_YT: EncodingPrefix = 3120;

    pub(super) const KNOWN_PREFIX: phf::OrderedMap<EncodingPrefix, &'static str> = phf_ordered_map! {
        1024u16 => "application/1d-interleaved-parityfec",
        1025u16 => "application/3gpdash-qoe-report+xml",
        1026u16 => "application/3gpp-ims+xml",
        1027u16 => "application/3gppHal+json",
        1028u16 => "application/3gppHalForms+json",
        1029u16 => "application/A2L",
        1030u16 => "application/AML",
        1031u16 => "application/ATF",
        1032u16 => "application/ATFX",
        1033u16 => "application/ATXML",
        1034u16 => "application/CALS-1840",
        1035u16 => "application/CDFX+XML",
        1036u16 => "application/CEA",
        1037u16 => "application/CSTAdata+xml",
        1038u16 => "application/DCD",
        1039u16 => "application/DII",
        1040u16 => "application/DIT",
        1041u16 => "application/EDI-X12",
        1042u16 => "application/EDI-consent",
        1043u16 => "application/EDIFACT",
        1044u16 => "application/EmergencyCallData.Comment+xml",
        1045u16 => "application/EmergencyCallData.Control+xml",
        1046u16 => "application/EmergencyCallData.DeviceInfo+xml",
        1047u16 => "application/EmergencyCallData.LegacyESN+json",
        1048u16 => "application/EmergencyCallData.ProviderInfo+xml",
        1049u16 => "application/EmergencyCallData.ServiceInfo+xml",
        1050u16 => "application/EmergencyCallData.SubscriberInfo+xml",
        1051u16 => "application/EmergencyCallData.VEDS+xml",
        1052u16 => "application/EmergencyCallData.cap+xml",
        1053u16 => "application/EmergencyCallData.eCall.MSD",
        1054u16 => "application/H224",
        1055u16 => "application/IOTP",
        1056u16 => "application/ISUP",
        1057u16 => "application/LXF",
        1058u16 => "application/MF4",
        1059u16 => "application/ODA",
        1060u16 => "application/ODX",
        1061u16 => "application/PDX",
        1062u16 => "application/QSIG",
        1063u16 => "application/SGML",
        1064u16 => "application/TETRA_ISI",
        1065u16 => "application/ace+cbor",
        1066u16 => "application/ace+json",
        1067u16 => "application/activemessage",
        1068u16 => "application/activity+json",
        1069u16 => "application/aif+cbor",
        1070u16 => "application/aif+json",
        1071u16 => "application/alto-cdni+json",
        1072u16 => "application/alto-cdnifilter+json",
        1073u16 => "application/alto-costmap+json",
        1074u16 => "application/alto-costmapfilter+json",
        1075u16 => "application/alto-directory+json",
        1076u16 => "application/alto-endpointcost+json",
        1077u16 => "application/alto-endpointcostparams+json",
        1078u16 => "application/alto-endpointprop+json",
        1079u16 => "application/alto-endpointpropparams+json",
        1080u16 => "application/alto-error+json",
        1081u16 => "application/alto-networkmap+json",
        1082u16 => "application/alto-networkmapfilter+json",
        1083u16 => "application/alto-propmap+json",
        1084u16 => "application/alto-propmapparams+json",
        1085u16 => "application/alto-tips+json",
        1086u16 => "application/alto-tipsparams+json",
        1087u16 => "application/alto-updatestreamcontrol+json",
        1088u16 => "application/alto-updatestreamparams+json",
        1089u16 => "application/andrew-inset",
        1090u16 => "application/applefile",
        1091u16 => "application/at+jwt",
        1092u16 => "application/atom+xml",
        1093u16 => "application/atomcat+xml",
        1094u16 => "application/atomdeleted+xml",
        1095u16 => "application/atomicmail",
        1096u16 => "application/atomsvc+xml",
        1097u16 => "application/atsc-dwd+xml",
        1098u16 => "application/atsc-dynamic-event-message",
        1099u16 => "application/atsc-held+xml",
        1100u16 => "application/atsc-rdt+json",
        1101u16 => "application/atsc-rsat+xml",
        1102u16 => "application/auth-policy+xml",
        1103u16 => "application/automationml-aml+xml",
        1104u16 => "application/automationml-amlx+zip",
        1105u16 => "application/bacnet-xdd+zip",
        1106u16 => "application/batch-SMTP",
        1107u16 => "application/beep+xml",
        1108u16 => "application/c2pa",
        1109u16 => "application/calendar+json",
        1110u16 => "application/calendar+xml",
        1111u16 => "application/call-completion",
        1112u16 => "application/captive+json",
        1113u16 => "application/cbor",
        1114u16 => "application/cbor-seq",
        1115u16 => "application/cccex",
        1116u16 => "application/ccmp+xml",
        1117u16 => "application/ccxml+xml",
        1118u16 => "application/cda+xml",
        1119u16 => "application/cdmi-capability",
        1120u16 => "application/cdmi-container",
        1121u16 => "application/cdmi-domain",
        1122u16 => "application/cdmi-object",
        1123u16 => "application/cdmi-queue",
        1124u16 => "application/cdni",
        1125u16 => "application/cea-2018+xml",
        1126u16 => "application/cellml+xml",
        1127u16 => "application/cfw",
        1128u16 => "application/cid-edhoc+cbor-seq",
        1129u16 => "application/city+json",
        1130u16 => "application/clr",
        1131u16 => "application/clue+xml",
        1132u16 => "application/clue_info+xml",
        1133u16 => "application/cms",
        1134u16 => "application/cnrp+xml",
        1135u16 => "application/coap-group+json",
        1136u16 => "application/coap-payload",
        1137u16 => "application/commonground",
        1138u16 => "application/concise-problem-details+cbor",
        1139u16 => "application/conference-info+xml",
        1140u16 => "application/cose",
        1141u16 => "application/cose-key",
        1142u16 => "application/cose-key-set",
        1143u16 => "application/cose-x509",
        1144u16 => "application/cpl+xml",
        1145u16 => "application/csrattrs",
        1146u16 => "application/csta+xml",
        1147u16 => "application/csvm+json",
        1148u16 => "application/cwl",
        1149u16 => "application/cwl+json",
        1150u16 => "application/cwt",
        1151u16 => "application/cybercash",
        1152u16 => "application/dash+xml",
        1153u16 => "application/dash-patch+xml",
        1154u16 => "application/dashdelta",
        1155u16 => "application/davmount+xml",
        1156u16 => "application/dca-rft",
        1157u16 => "application/dec-dx",
        1158u16 => "application/dialog-info+xml",
        1159u16 => "application/dicom",
        1160u16 => "application/dicom+json",
        1161u16 => "application/dicom+xml",
        1162u16 => "application/dns",
        1163u16 => "application/dns+json",
        1164u16 => "application/dns-message",
        1165u16 => "application/dots+cbor",
        1166u16 => "application/dpop+jwt",
        1167u16 => "application/dskpp+xml",
        1168u16 => "application/dssc+der",
        1169u16 => "application/dssc+xml",
        1170u16 => "application/dvcs",
        1171u16 => "application/ecmascript",
        1172u16 => "application/edhoc+cbor-seq",
        1173u16 => "application/efi",
        1174u16 => "application/elm+json",
        1175u16 => "application/elm+xml",
        1176u16 => "application/emma+xml",
        1177u16 => "application/emotionml+xml",
        1178u16 => "application/encaprtp",
        1179u16 => "application/epp+xml",
        1180u16 => "application/epub+zip",
        1181u16 => "application/eshop",
        1182u16 => "application/example",
        1183u16 => "application/exi",
        1184u16 => "application/expect-ct-report+json",
        1185u16 => "application/express",
        1186u16 => "application/fastinfoset",
        1187u16 => "application/fastsoap",
        1188u16 => "application/fdf",
        1189u16 => "application/fdt+xml",
        1190u16 => "application/fhir+json",
        1191u16 => "application/fhir+xml",
        1192u16 => "application/fits",
        1193u16 => "application/flexfec",
        1194u16 => "application/font-sfnt",
        1195u16 => "application/font-tdpfr",
        1196u16 => "application/font-woff",
        1197u16 => "application/framework-attributes+xml",
        1198u16 => "application/geo+json",
        1199u16 => "application/geo+json-seq",
        1200u16 => "application/geopackage+sqlite3",
        1201u16 => "application/geoxacml+json",
        1202u16 => "application/geoxacml+xml",
        1203u16 => "application/gltf-buffer",
        1204u16 => "application/gml+xml",
        1205u16 => "application/gzip",
        1206u16 => "application/held+xml",
        1207u16 => "application/hl7v2+xml",
        1208u16 => "application/http",
        1209u16 => "application/hyperstudio",
        1210u16 => "application/ibe-key-request+xml",
        1211u16 => "application/ibe-pkg-reply+xml",
        1212u16 => "application/ibe-pp-data",
        1213u16 => "application/iges",
        1214u16 => "application/im-iscomposing+xml",
        1215u16 => "application/index",
        1216u16 => "application/index.cmd",
        1217u16 => "application/index.obj",
        1218u16 => "application/index.response",
        1219u16 => "application/index.vnd",
        1220u16 => "application/inkml+xml",
        1221u16 => "application/ipfix",
        1222u16 => "application/ipp",
        1223u16 => "application/its+xml",
        1224u16 => "application/java-archive",
        1225u16 => "application/javascript",
        1226u16 => "application/jf2feed+json",
        1227u16 => "application/jose",
        1228u16 => "application/jose+json",
        1229u16 => "application/jrd+json",
        1230u16 => "application/jscalendar+json",
        1231u16 => "application/jscontact+json",
        1232u16 => "application/json",
        1233u16 => "application/json-patch+json",
        1234u16 => "application/json-seq",
        1235u16 => "application/jsonpath",
        1236u16 => "application/jwk+json",
        1237u16 => "application/jwk-set+json",
        1238u16 => "application/jwt",
        1239u16 => "application/kpml-request+xml",
        1240u16 => "application/kpml-response+xml",
        1241u16 => "application/ld+json",
        1242u16 => "application/lgr+xml",
        1243u16 => "application/link-format",
        1244u16 => "application/linkset",
        1245u16 => "application/linkset+json",
        1246u16 => "application/load-control+xml",
        1247u16 => "application/logout+jwt",
        1248u16 => "application/lost+xml",
        1249u16 => "application/lostsync+xml",
        1250u16 => "application/lpf+zip",
        1251u16 => "application/mac-binhex40",
        1252u16 => "application/macwriteii",
        1253u16 => "application/mads+xml",
        1254u16 => "application/manifest+json",
        1255u16 => "application/marc",
        1256u16 => "application/marcxml+xml",
        1257u16 => "application/mathematica",
        1258u16 => "application/mathml+xml",
        1259u16 => "application/mathml-content+xml",
        1260u16 => "application/mathml-presentation+xml",
        1261u16 => "application/mbms-associated-procedure-description+xml",
        1262u16 => "application/mbms-deregister+xml",
        1263u16 => "application/mbms-envelope+xml",
        1264u16 => "application/mbms-msk+xml",
        1265u16 => "application/mbms-msk-response+xml",
        1266u16 => "application/mbms-protection-description+xml",
        1267u16 => "application/mbms-reception-report+xml",
        1268u16 => "application/mbms-register+xml",
        1269u16 => "application/mbms-register-response+xml",
        1270u16 => "application/mbms-schedule+xml",
        1271u16 => "application/mbms-user-service-description+xml",
        1272u16 => "application/mbox",
        1273u16 => "application/media-policy-dataset+xml",
        1274u16 => "application/media_control+xml",
        1275u16 => "application/mediaservercontrol+xml",
        1276u16 => "application/merge-patch+json",
        1277u16 => "application/metalink4+xml",
        1278u16 => "application/mets+xml",
        1279u16 => "application/mikey",
        1280u16 => "application/mipc",
        1281u16 => "application/missing-blocks+cbor-seq",
        1282u16 => "application/mmt-aei+xml",
        1283u16 => "application/mmt-usd+xml",
        1284u16 => "application/mods+xml",
        1285u16 => "application/moss-keys",
        1286u16 => "application/moss-signature",
        1287u16 => "application/mosskey-data",
        1288u16 => "application/mosskey-request",
        1289u16 => "application/mp21",
        1290u16 => "application/mp4",
        1291u16 => "application/mpeg4-generic",
        1292u16 => "application/mpeg4-iod",
        1293u16 => "application/mpeg4-iod-xmt",
        1294u16 => "application/mrb-consumer+xml",
        1295u16 => "application/mrb-publish+xml",
        1296u16 => "application/msc-ivr+xml",
        1297u16 => "application/msc-mixer+xml",
        1298u16 => "application/msword",
        1299u16 => "application/mud+json",
        1300u16 => "application/multipart-core",
        1301u16 => "application/mxf",
        1302u16 => "application/n-quads",
        1303u16 => "application/n-triples",
        1304u16 => "application/nasdata",
        1305u16 => "application/news-checkgroups",
        1306u16 => "application/news-groupinfo",
        1307u16 => "application/news-transmission",
        1308u16 => "application/nlsml+xml",
        1309u16 => "application/node",
        1310u16 => "application/nss",
        1311u16 => "application/oauth-authz-req+jwt",
        1312u16 => "application/oblivious-dns-message",
        1313u16 => "application/ocsp-request",
        1314u16 => "application/ocsp-response",
        1315u16 => "application/octet-stream",
        1316u16 => "application/odm+xml",
        1317u16 => "application/oebps-package+xml",
        1318u16 => "application/ogg",
        1319u16 => "application/ohttp-keys",
        1320u16 => "application/opc-nodeset+xml",
        1321u16 => "application/oscore",
        1322u16 => "application/oxps",
        1323u16 => "application/p21",
        1324u16 => "application/p21+zip",
        1325u16 => "application/p2p-overlay+xml",
        1326u16 => "application/parityfec",
        1327u16 => "application/passport",
        1328u16 => "application/patch-ops-error+xml",
        1329u16 => "application/pdf",
        1330u16 => "application/pem-certificate-chain",
        1331u16 => "application/pgp-encrypted",
        1332u16 => "application/pgp-keys",
        1333u16 => "application/pgp-signature",
        1334u16 => "application/pidf+xml",
        1335u16 => "application/pidf-diff+xml",
        1336u16 => "application/pkcs10",
        1337u16 => "application/pkcs12",
        1338u16 => "application/pkcs7-mime",
        1339u16 => "application/pkcs7-signature",
        1340u16 => "application/pkcs8",
        1341u16 => "application/pkcs8-encrypted",
        1342u16 => "application/pkix-attr-cert",
        1343u16 => "application/pkix-cert",
        1344u16 => "application/pkix-crl",
        1345u16 => "application/pkix-pkipath",
        1346u16 => "application/pkixcmp",
        1347u16 => "application/pls+xml",
        1348u16 => "application/poc-settings+xml",
        1349u16 => "application/postscript",
        1350u16 => "application/ppsp-tracker+json",
        1351u16 => "application/private-token-issuer-directory",
        1352u16 => "application/private-token-request",
        1353u16 => "application/private-token-response",
        1354u16 => "application/problem+json",
        1355u16 => "application/problem+xml",
        1356u16 => "application/provenance+xml",
        1357u16 => "application/prs.alvestrand.titrax-sheet",
        1358u16 => "application/prs.cww",
        1359u16 => "application/prs.cyn",
        1360u16 => "application/prs.hpub+zip",
        1361u16 => "application/prs.implied-document+xml",
        1362u16 => "application/prs.implied-executable",
        1363u16 => "application/prs.implied-object+json",
        1364u16 => "application/prs.implied-object+json-seq",
        1365u16 => "application/prs.implied-object+yaml",
        1366u16 => "application/prs.implied-structure",
        1367u16 => "application/prs.nprend",
        1368u16 => "application/prs.plucker",
        1369u16 => "application/prs.rdf-xml-crypt",
        1370u16 => "application/prs.vcfbzip2",
        1371u16 => "application/prs.xsf+xml",
        1372u16 => "application/pskc+xml",
        1373u16 => "application/pvd+json",
        1374u16 => "application/raptorfec",
        1375u16 => "application/rdap+json",
        1376u16 => "application/rdf+xml",
        1377u16 => "application/reginfo+xml",
        1378u16 => "application/relax-ng-compact-syntax",
        1379u16 => "application/remote-printing",
        1380u16 => "application/reputon+json",
        1381u16 => "application/resource-lists+xml",
        1382u16 => "application/resource-lists-diff+xml",
        1383u16 => "application/rfc+xml",
        1384u16 => "application/riscos",
        1385u16 => "application/rlmi+xml",
        1386u16 => "application/rls-services+xml",
        1387u16 => "application/route-apd+xml",
        1388u16 => "application/route-s-tsid+xml",
        1389u16 => "application/route-usd+xml",
        1390u16 => "application/rpki-checklist",
        1391u16 => "application/rpki-ghostbusters",
        1392u16 => "application/rpki-manifest",
        1393u16 => "application/rpki-publication",
        1394u16 => "application/rpki-roa",
        1395u16 => "application/rpki-updown",
        1396u16 => "application/rtf",
        1397u16 => "application/rtploopback",
        1398u16 => "application/rtx",
        1399u16 => "application/samlassertion+xml",
        1400u16 => "application/samlmetadata+xml",
        1401u16 => "application/sarif+json",
        1402u16 => "application/sarif-external-properties+json",
        1403u16 => "application/sbe",
        1404u16 => "application/sbml+xml",
        1405u16 => "application/scaip+xml",
        1406u16 => "application/scim+json",
        1407u16 => "application/scvp-cv-request",
        1408u16 => "application/scvp-cv-response",
        1409u16 => "application/scvp-vp-request",
        1410u16 => "application/scvp-vp-response",
        1411u16 => "application/sdp",
        1412u16 => "application/secevent+jwt",
        1413u16 => "application/senml+cbor",
        1414u16 => "application/senml+json",
        1415u16 => "application/senml+xml",
        1416u16 => "application/senml-etch+cbor",
        1417u16 => "application/senml-etch+json",
        1418u16 => "application/senml-exi",
        1419u16 => "application/sensml+cbor",
        1420u16 => "application/sensml+json",
        1421u16 => "application/sensml+xml",
        1422u16 => "application/sensml-exi",
        1423u16 => "application/sep+xml",
        1424u16 => "application/sep-exi",
        1425u16 => "application/session-info",
        1426u16 => "application/set-payment",
        1427u16 => "application/set-payment-initiation",
        1428u16 => "application/set-registration",
        1429u16 => "application/set-registration-initiation",
        1430u16 => "application/sgml-open-catalog",
        1431u16 => "application/shf+xml",
        1432u16 => "application/sieve",
        1433u16 => "application/simple-filter+xml",
        1434u16 => "application/simple-message-summary",
        1435u16 => "application/simpleSymbolContainer",
        1436u16 => "application/sipc",
        1437u16 => "application/slate",
        1438u16 => "application/smil",
        1439u16 => "application/smil+xml",
        1440u16 => "application/smpte336m",
        1441u16 => "application/soap+fastinfoset",
        1442u16 => "application/soap+xml",
        1443u16 => "application/sparql-query",
        1444u16 => "application/sparql-results+xml",
        1445u16 => "application/spdx+json",
        1446u16 => "application/spirits-event+xml",
        1447u16 => "application/sql",
        1448u16 => "application/srgs",
        1449u16 => "application/srgs+xml",
        1450u16 => "application/sru+xml",
        1451u16 => "application/ssml+xml",
        1452u16 => "application/stix+json",
        1453u16 => "application/swid+cbor",
        1454u16 => "application/swid+xml",
        1455u16 => "application/tamp-apex-update",
        1456u16 => "application/tamp-apex-update-confirm",
        1457u16 => "application/tamp-community-update",
        1458u16 => "application/tamp-community-update-confirm",
        1459u16 => "application/tamp-error",
        1460u16 => "application/tamp-sequence-adjust",
        1461u16 => "application/tamp-sequence-adjust-confirm",
        1462u16 => "application/tamp-status-query",
        1463u16 => "application/tamp-status-response",
        1464u16 => "application/tamp-update",
        1465u16 => "application/tamp-update-confirm",
        1466u16 => "application/taxii+json",
        1467u16 => "application/td+json",
        1468u16 => "application/tei+xml",
        1469u16 => "application/thraud+xml",
        1470u16 => "application/timestamp-query",
        1471u16 => "application/timestamp-reply",
        1472u16 => "application/timestamped-data",
        1473u16 => "application/tlsrpt+gzip",
        1474u16 => "application/tlsrpt+json",
        1475u16 => "application/tm+json",
        1476u16 => "application/tnauthlist",
        1477u16 => "application/token-introspection+jwt",
        1478u16 => "application/trickle-ice-sdpfrag",
        1479u16 => "application/trig",
        1480u16 => "application/ttml+xml",
        1481u16 => "application/tve-trigger",
        1482u16 => "application/tzif",
        1483u16 => "application/tzif-leap",
        1484u16 => "application/ulpfec",
        1485u16 => "application/urc-grpsheet+xml",
        1486u16 => "application/urc-ressheet+xml",
        1487u16 => "application/urc-targetdesc+xml",
        1488u16 => "application/urc-uisocketdesc+xml",
        1489u16 => "application/vcard+json",
        1490u16 => "application/vcard+xml",
        1491u16 => "application/vemmi",
        1492u16 => "application/vnd.1000minds.decision-model+xml",
        1493u16 => "application/vnd.1ob",
        1494u16 => "application/vnd.3M.Post-it-Notes",
        1495u16 => "application/vnd.3gpp-prose+xml",
        1496u16 => "application/vnd.3gpp-prose-pc3a+xml",
        1497u16 => "application/vnd.3gpp-prose-pc3ach+xml",
        1498u16 => "application/vnd.3gpp-prose-pc3ch+xml",
        1499u16 => "application/vnd.3gpp-prose-pc8+xml",
        1500u16 => "application/vnd.3gpp-v2x-local-service-information",
        1501u16 => "application/vnd.3gpp.5gnas",
        1502u16 => "application/vnd.3gpp.GMOP+xml",
        1503u16 => "application/vnd.3gpp.SRVCC-info+xml",
        1504u16 => "application/vnd.3gpp.access-transfer-events+xml",
        1505u16 => "application/vnd.3gpp.bsf+xml",
        1506u16 => "application/vnd.3gpp.crs+xml",
        1507u16 => "application/vnd.3gpp.current-location-discovery+xml",
        1508u16 => "application/vnd.3gpp.gtpc",
        1509u16 => "application/vnd.3gpp.interworking-data",
        1510u16 => "application/vnd.3gpp.lpp",
        1511u16 => "application/vnd.3gpp.mc-signalling-ear",
        1512u16 => "application/vnd.3gpp.mcdata-affiliation-command+xml",
        1513u16 => "application/vnd.3gpp.mcdata-info+xml",
        1514u16 => "application/vnd.3gpp.mcdata-msgstore-ctrl-request+xml",
        1515u16 => "application/vnd.3gpp.mcdata-payload",
        1516u16 => "application/vnd.3gpp.mcdata-regroup+xml",
        1517u16 => "application/vnd.3gpp.mcdata-service-config+xml",
        1518u16 => "application/vnd.3gpp.mcdata-signalling",
        1519u16 => "application/vnd.3gpp.mcdata-ue-config+xml",
        1520u16 => "application/vnd.3gpp.mcdata-user-profile+xml",
        1521u16 => "application/vnd.3gpp.mcptt-affiliation-command+xml",
        1522u16 => "application/vnd.3gpp.mcptt-floor-request+xml",
        1523u16 => "application/vnd.3gpp.mcptt-info+xml",
        1524u16 => "application/vnd.3gpp.mcptt-location-info+xml",
        1525u16 => "application/vnd.3gpp.mcptt-mbms-usage-info+xml",
        1526u16 => "application/vnd.3gpp.mcptt-regroup+xml",
        1527u16 => "application/vnd.3gpp.mcptt-service-config+xml",
        1528u16 => "application/vnd.3gpp.mcptt-signed+xml",
        1529u16 => "application/vnd.3gpp.mcptt-ue-config+xml",
        1530u16 => "application/vnd.3gpp.mcptt-ue-init-config+xml",
        1531u16 => "application/vnd.3gpp.mcptt-user-profile+xml",
        1532u16 => "application/vnd.3gpp.mcvideo-affiliation-command+xml",
        1533u16 => "application/vnd.3gpp.mcvideo-affiliation-info+xml",
        1534u16 => "application/vnd.3gpp.mcvideo-info+xml",
        1535u16 => "application/vnd.3gpp.mcvideo-location-info+xml",
        1536u16 => "application/vnd.3gpp.mcvideo-mbms-usage-info+xml",
        1537u16 => "application/vnd.3gpp.mcvideo-regroup+xml",
        1538u16 => "application/vnd.3gpp.mcvideo-service-config+xml",
        1539u16 => "application/vnd.3gpp.mcvideo-transmission-request+xml",
        1540u16 => "application/vnd.3gpp.mcvideo-ue-config+xml",
        1541u16 => "application/vnd.3gpp.mcvideo-user-profile+xml",
        1542u16 => "application/vnd.3gpp.mid-call+xml",
        1543u16 => "application/vnd.3gpp.ngap",
        1544u16 => "application/vnd.3gpp.pfcp",
        1545u16 => "application/vnd.3gpp.pic-bw-large",
        1546u16 => "application/vnd.3gpp.pic-bw-small",
        1547u16 => "application/vnd.3gpp.pic-bw-var",
        1548u16 => "application/vnd.3gpp.s1ap",
        1549u16 => "application/vnd.3gpp.seal-group-doc+xml",
        1550u16 => "application/vnd.3gpp.seal-info+xml",
        1551u16 => "application/vnd.3gpp.seal-location-info+xml",
        1552u16 => "application/vnd.3gpp.seal-mbms-usage-info+xml",
        1553u16 => "application/vnd.3gpp.seal-network-QoS-management-info+xml",
        1554u16 => "application/vnd.3gpp.seal-ue-config-info+xml",
        1555u16 => "application/vnd.3gpp.seal-unicast-info+xml",
        1556u16 => "application/vnd.3gpp.seal-user-profile-info+xml",
        1557u16 => "application/vnd.3gpp.sms",
        1558u16 => "application/vnd.3gpp.sms+xml",
        1559u16 => "application/vnd.3gpp.srvcc-ext+xml",
        1560u16 => "application/vnd.3gpp.state-and-event-info+xml",
        1561u16 => "application/vnd.3gpp.ussd+xml",
        1562u16 => "application/vnd.3gpp.v2x",
        1563u16 => "application/vnd.3gpp.vae-info+xml",
        1564u16 => "application/vnd.3gpp2.bcmcsinfo+xml",
        1565u16 => "application/vnd.3gpp2.sms",
        1566u16 => "application/vnd.3gpp2.tcap",
        1567u16 => "application/vnd.3lightssoftware.imagescal",
        1568u16 => "application/vnd.FloGraphIt",
        1569u16 => "application/vnd.HandHeld-Entertainment+xml",
        1570u16 => "application/vnd.Kinar",
        1571u16 => "application/vnd.MFER",
        1572u16 => "application/vnd.Mobius.DAF",
        1573u16 => "application/vnd.Mobius.DIS",
        1574u16 => "application/vnd.Mobius.MBK",
        1575u16 => "application/vnd.Mobius.MQY",
        1576u16 => "application/vnd.Mobius.MSL",
        1577u16 => "application/vnd.Mobius.PLC",
        1578u16 => "application/vnd.Mobius.TXF",
        1579u16 => "application/vnd.Quark.QuarkXPress",
        1580u16 => "application/vnd.RenLearn.rlprint",
        1581u16 => "application/vnd.SimTech-MindMapper",
        1582u16 => "application/vnd.accpac.simply.aso",
        1583u16 => "application/vnd.accpac.simply.imp",
        1584u16 => "application/vnd.acm.addressxfer+json",
        1585u16 => "application/vnd.acm.chatbot+json",
        1586u16 => "application/vnd.acucobol",
        1587u16 => "application/vnd.acucorp",
        1588u16 => "application/vnd.adobe.flash.movie",
        1589u16 => "application/vnd.adobe.formscentral.fcdt",
        1590u16 => "application/vnd.adobe.fxp",
        1591u16 => "application/vnd.adobe.partial-upload",
        1592u16 => "application/vnd.adobe.xdp+xml",
        1593u16 => "application/vnd.aether.imp",
        1594u16 => "application/vnd.afpc.afplinedata",
        1595u16 => "application/vnd.afpc.afplinedata-pagedef",
        1596u16 => "application/vnd.afpc.cmoca-cmresource",
        1597u16 => "application/vnd.afpc.foca-charset",
        1598u16 => "application/vnd.afpc.foca-codedfont",
        1599u16 => "application/vnd.afpc.foca-codepage",
        1600u16 => "application/vnd.afpc.modca",
        1601u16 => "application/vnd.afpc.modca-cmtable",
        1602u16 => "application/vnd.afpc.modca-formdef",
        1603u16 => "application/vnd.afpc.modca-mediummap",
        1604u16 => "application/vnd.afpc.modca-objectcontainer",
        1605u16 => "application/vnd.afpc.modca-overlay",
        1606u16 => "application/vnd.afpc.modca-pagesegment",
        1607u16 => "application/vnd.age",
        1608u16 => "application/vnd.ah-barcode",
        1609u16 => "application/vnd.ahead.space",
        1610u16 => "application/vnd.airzip.filesecure.azf",
        1611u16 => "application/vnd.airzip.filesecure.azs",
        1612u16 => "application/vnd.amadeus+json",
        1613u16 => "application/vnd.amazon.mobi8-ebook",
        1614u16 => "application/vnd.americandynamics.acc",
        1615u16 => "application/vnd.amiga.ami",
        1616u16 => "application/vnd.amundsen.maze+xml",
        1617u16 => "application/vnd.android.ota",
        1618u16 => "application/vnd.anki",
        1619u16 => "application/vnd.anser-web-certificate-issue-initiation",
        1620u16 => "application/vnd.antix.game-component",
        1621u16 => "application/vnd.apache.arrow.file",
        1622u16 => "application/vnd.apache.arrow.stream",
        1623u16 => "application/vnd.apache.parquet",
        1624u16 => "application/vnd.apache.thrift.binary",
        1625u16 => "application/vnd.apache.thrift.compact",
        1626u16 => "application/vnd.apache.thrift.json",
        1627u16 => "application/vnd.apexlang",
        1628u16 => "application/vnd.api+json",
        1629u16 => "application/vnd.aplextor.warrp+json",
        1630u16 => "application/vnd.apothekende.reservation+json",
        1631u16 => "application/vnd.apple.installer+xml",
        1632u16 => "application/vnd.apple.keynote",
        1633u16 => "application/vnd.apple.mpegurl",
        1634u16 => "application/vnd.apple.numbers",
        1635u16 => "application/vnd.apple.pages",
        1636u16 => "application/vnd.arastra.swi",
        1637u16 => "application/vnd.aristanetworks.swi",
        1638u16 => "application/vnd.artisan+json",
        1639u16 => "application/vnd.artsquare",
        1640u16 => "application/vnd.astraea-software.iota",
        1641u16 => "application/vnd.audiograph",
        1642u16 => "application/vnd.autopackage",
        1643u16 => "application/vnd.avalon+json",
        1644u16 => "application/vnd.avistar+xml",
        1645u16 => "application/vnd.balsamiq.bmml+xml",
        1646u16 => "application/vnd.balsamiq.bmpr",
        1647u16 => "application/vnd.banana-accounting",
        1648u16 => "application/vnd.bbf.usp.error",
        1649u16 => "application/vnd.bbf.usp.msg",
        1650u16 => "application/vnd.bbf.usp.msg+json",
        1651u16 => "application/vnd.bekitzur-stech+json",
        1652u16 => "application/vnd.belightsoft.lhzd+zip",
        1653u16 => "application/vnd.belightsoft.lhzl+zip",
        1654u16 => "application/vnd.bint.med-content",
        1655u16 => "application/vnd.biopax.rdf+xml",
        1656u16 => "application/vnd.blink-idb-value-wrapper",
        1657u16 => "application/vnd.blueice.multipass",
        1658u16 => "application/vnd.bluetooth.ep.oob",
        1659u16 => "application/vnd.bluetooth.le.oob",
        1660u16 => "application/vnd.bmi",
        1661u16 => "application/vnd.bpf",
        1662u16 => "application/vnd.bpf3",
        1663u16 => "application/vnd.businessobjects",
        1664u16 => "application/vnd.byu.uapi+json",
        1665u16 => "application/vnd.bzip3",
        1666u16 => "application/vnd.cab-jscript",
        1667u16 => "application/vnd.canon-cpdl",
        1668u16 => "application/vnd.canon-lips",
        1669u16 => "application/vnd.capasystems-pg+json",
        1670u16 => "application/vnd.cendio.thinlinc.clientconf",
        1671u16 => "application/vnd.century-systems.tcp_stream",
        1672u16 => "application/vnd.chemdraw+xml",
        1673u16 => "application/vnd.chess-pgn",
        1674u16 => "application/vnd.chipnuts.karaoke-mmd",
        1675u16 => "application/vnd.ciedi",
        1676u16 => "application/vnd.cinderella",
        1677u16 => "application/vnd.cirpack.isdn-ext",
        1678u16 => "application/vnd.citationstyles.style+xml",
        1679u16 => "application/vnd.claymore",
        1680u16 => "application/vnd.cloanto.rp9",
        1681u16 => "application/vnd.clonk.c4group",
        1682u16 => "application/vnd.cluetrust.cartomobile-config",
        1683u16 => "application/vnd.cluetrust.cartomobile-config-pkg",
        1684u16 => "application/vnd.cncf.helm.chart.content.v1.tar+gzip",
        1685u16 => "application/vnd.cncf.helm.chart.provenance.v1.prov",
        1686u16 => "application/vnd.cncf.helm.config.v1+json",
        1687u16 => "application/vnd.coffeescript",
        1688u16 => "application/vnd.collabio.xodocuments.document",
        1689u16 => "application/vnd.collabio.xodocuments.document-template",
        1690u16 => "application/vnd.collabio.xodocuments.presentation",
        1691u16 => "application/vnd.collabio.xodocuments.presentation-template",
        1692u16 => "application/vnd.collabio.xodocuments.spreadsheet",
        1693u16 => "application/vnd.collabio.xodocuments.spreadsheet-template",
        1694u16 => "application/vnd.collection+json",
        1695u16 => "application/vnd.collection.doc+json",
        1696u16 => "application/vnd.collection.next+json",
        1697u16 => "application/vnd.comicbook+zip",
        1698u16 => "application/vnd.comicbook-rar",
        1699u16 => "application/vnd.commerce-battelle",
        1700u16 => "application/vnd.commonspace",
        1701u16 => "application/vnd.contact.cmsg",
        1702u16 => "application/vnd.coreos.ignition+json",
        1703u16 => "application/vnd.cosmocaller",
        1704u16 => "application/vnd.crick.clicker",
        1705u16 => "application/vnd.crick.clicker.keyboard",
        1706u16 => "application/vnd.crick.clicker.palette",
        1707u16 => "application/vnd.crick.clicker.template",
        1708u16 => "application/vnd.crick.clicker.wordbank",
        1709u16 => "application/vnd.criticaltools.wbs+xml",
        1710u16 => "application/vnd.cryptii.pipe+json",
        1711u16 => "application/vnd.crypto-shade-file",
        1712u16 => "application/vnd.cryptomator.encrypted",
        1713u16 => "application/vnd.cryptomator.vault",
        1714u16 => "application/vnd.ctc-posml",
        1715u16 => "application/vnd.ctct.ws+xml",
        1716u16 => "application/vnd.cups-pdf",
        1717u16 => "application/vnd.cups-postscript",
        1718u16 => "application/vnd.cups-ppd",
        1719u16 => "application/vnd.cups-raster",
        1720u16 => "application/vnd.cups-raw",
        1721u16 => "application/vnd.curl",
        1722u16 => "application/vnd.cyan.dean.root+xml",
        1723u16 => "application/vnd.cybank",
        1724u16 => "application/vnd.cyclonedx+json",
        1725u16 => "application/vnd.cyclonedx+xml",
        1726u16 => "application/vnd.d2l.coursepackage1p0+zip",
        1727u16 => "application/vnd.d3m-dataset",
        1728u16 => "application/vnd.d3m-problem",
        1729u16 => "application/vnd.dart",
        1730u16 => "application/vnd.data-vision.rdz",
        1731u16 => "application/vnd.datalog",
        1732u16 => "application/vnd.datapackage+json",
        1733u16 => "application/vnd.dataresource+json",
        1734u16 => "application/vnd.dbf",
        1735u16 => "application/vnd.debian.binary-package",
        1736u16 => "application/vnd.dece.data",
        1737u16 => "application/vnd.dece.ttml+xml",
        1738u16 => "application/vnd.dece.unspecified",
        1739u16 => "application/vnd.dece.zip",
        1740u16 => "application/vnd.denovo.fcselayout-link",
        1741u16 => "application/vnd.desmume.movie",
        1742u16 => "application/vnd.dir-bi.plate-dl-nosuffix",
        1743u16 => "application/vnd.dm.delegation+xml",
        1744u16 => "application/vnd.dna",
        1745u16 => "application/vnd.document+json",
        1746u16 => "application/vnd.dolby.mobile.1",
        1747u16 => "application/vnd.dolby.mobile.2",
        1748u16 => "application/vnd.doremir.scorecloud-binary-document",
        1749u16 => "application/vnd.dpgraph",
        1750u16 => "application/vnd.dreamfactory",
        1751u16 => "application/vnd.drive+json",
        1752u16 => "application/vnd.dtg.local",
        1753u16 => "application/vnd.dtg.local.flash",
        1754u16 => "application/vnd.dtg.local.html",
        1755u16 => "application/vnd.dvb.ait",
        1756u16 => "application/vnd.dvb.dvbisl+xml",
        1757u16 => "application/vnd.dvb.dvbj",
        1758u16 => "application/vnd.dvb.esgcontainer",
        1759u16 => "application/vnd.dvb.ipdcdftnotifaccess",
        1760u16 => "application/vnd.dvb.ipdcesgaccess",
        1761u16 => "application/vnd.dvb.ipdcesgaccess2",
        1762u16 => "application/vnd.dvb.ipdcesgpdd",
        1763u16 => "application/vnd.dvb.ipdcroaming",
        1764u16 => "application/vnd.dvb.iptv.alfec-base",
        1765u16 => "application/vnd.dvb.iptv.alfec-enhancement",
        1766u16 => "application/vnd.dvb.notif-aggregate-root+xml",
        1767u16 => "application/vnd.dvb.notif-container+xml",
        1768u16 => "application/vnd.dvb.notif-generic+xml",
        1769u16 => "application/vnd.dvb.notif-ia-msglist+xml",
        1770u16 => "application/vnd.dvb.notif-ia-registration-request+xml",
        1771u16 => "application/vnd.dvb.notif-ia-registration-response+xml",
        1772u16 => "application/vnd.dvb.notif-init+xml",
        1773u16 => "application/vnd.dvb.pfr",
        1774u16 => "application/vnd.dvb.service",
        1775u16 => "application/vnd.dxr",
        1776u16 => "application/vnd.dynageo",
        1777u16 => "application/vnd.dzr",
        1778u16 => "application/vnd.easykaraoke.cdgdownload",
        1779u16 => "application/vnd.ecdis-update",
        1780u16 => "application/vnd.ecip.rlp",
        1781u16 => "application/vnd.eclipse.ditto+json",
        1782u16 => "application/vnd.ecowin.chart",
        1783u16 => "application/vnd.ecowin.filerequest",
        1784u16 => "application/vnd.ecowin.fileupdate",
        1785u16 => "application/vnd.ecowin.series",
        1786u16 => "application/vnd.ecowin.seriesrequest",
        1787u16 => "application/vnd.ecowin.seriesupdate",
        1788u16 => "application/vnd.efi.img",
        1789u16 => "application/vnd.efi.iso",
        1790u16 => "application/vnd.eln+zip",
        1791u16 => "application/vnd.emclient.accessrequest+xml",
        1792u16 => "application/vnd.enliven",
        1793u16 => "application/vnd.enphase.envoy",
        1794u16 => "application/vnd.eprints.data+xml",
        1795u16 => "application/vnd.epson.esf",
        1796u16 => "application/vnd.epson.msf",
        1797u16 => "application/vnd.epson.quickanime",
        1798u16 => "application/vnd.epson.salt",
        1799u16 => "application/vnd.epson.ssf",
        1800u16 => "application/vnd.ericsson.quickcall",
        1801u16 => "application/vnd.erofs",
        1802u16 => "application/vnd.espass-espass+zip",
        1803u16 => "application/vnd.eszigno3+xml",
        1804u16 => "application/vnd.etsi.aoc+xml",
        1805u16 => "application/vnd.etsi.asic-e+zip",
        1806u16 => "application/vnd.etsi.asic-s+zip",
        1807u16 => "application/vnd.etsi.cug+xml",
        1808u16 => "application/vnd.etsi.iptvcommand+xml",
        1809u16 => "application/vnd.etsi.iptvdiscovery+xml",
        1810u16 => "application/vnd.etsi.iptvprofile+xml",
        1811u16 => "application/vnd.etsi.iptvsad-bc+xml",
        1812u16 => "application/vnd.etsi.iptvsad-cod+xml",
        1813u16 => "application/vnd.etsi.iptvsad-npvr+xml",
        1814u16 => "application/vnd.etsi.iptvservice+xml",
        1815u16 => "application/vnd.etsi.iptvsync+xml",
        1816u16 => "application/vnd.etsi.iptvueprofile+xml",
        1817u16 => "application/vnd.etsi.mcid+xml",
        1818u16 => "application/vnd.etsi.mheg5",
        1819u16 => "application/vnd.etsi.overload-control-policy-dataset+xml",
        1820u16 => "application/vnd.etsi.pstn+xml",
        1821u16 => "application/vnd.etsi.sci+xml",
        1822u16 => "application/vnd.etsi.simservs+xml",
        1823u16 => "application/vnd.etsi.timestamp-token",
        1824u16 => "application/vnd.etsi.tsl+xml",
        1825u16 => "application/vnd.etsi.tsl.der",
        1826u16 => "application/vnd.eu.kasparian.car+json",
        1827u16 => "application/vnd.eudora.data",
        1828u16 => "application/vnd.evolv.ecig.profile",
        1829u16 => "application/vnd.evolv.ecig.settings",
        1830u16 => "application/vnd.evolv.ecig.theme",
        1831u16 => "application/vnd.exstream-empower+zip",
        1832u16 => "application/vnd.exstream-package",
        1833u16 => "application/vnd.ezpix-album",
        1834u16 => "application/vnd.ezpix-package",
        1835u16 => "application/vnd.f-secure.mobile",
        1836u16 => "application/vnd.familysearch.gedcom+zip",
        1837u16 => "application/vnd.fastcopy-disk-image",
        1838u16 => "application/vnd.fdsn.mseed",
        1839u16 => "application/vnd.fdsn.seed",
        1840u16 => "application/vnd.ffsns",
        1841u16 => "application/vnd.ficlab.flb+zip",
        1842u16 => "application/vnd.filmit.zfc",
        1843u16 => "application/vnd.fints",
        1844u16 => "application/vnd.firemonkeys.cloudcell",
        1845u16 => "application/vnd.fluxtime.clip",
        1846u16 => "application/vnd.font-fontforge-sfd",
        1847u16 => "application/vnd.framemaker",
        1848u16 => "application/vnd.freelog.comic",
        1849u16 => "application/vnd.frogans.fnc",
        1850u16 => "application/vnd.frogans.ltf",
        1851u16 => "application/vnd.fsc.weblaunch",
        1852u16 => "application/vnd.fujifilm.fb.docuworks",
        1853u16 => "application/vnd.fujifilm.fb.docuworks.binder",
        1854u16 => "application/vnd.fujifilm.fb.docuworks.container",
        1855u16 => "application/vnd.fujifilm.fb.jfi+xml",
        1856u16 => "application/vnd.fujitsu.oasys",
        1857u16 => "application/vnd.fujitsu.oasys2",
        1858u16 => "application/vnd.fujitsu.oasys3",
        1859u16 => "application/vnd.fujitsu.oasysgp",
        1860u16 => "application/vnd.fujitsu.oasysprs",
        1861u16 => "application/vnd.fujixerox.ART-EX",
        1862u16 => "application/vnd.fujixerox.ART4",
        1863u16 => "application/vnd.fujixerox.HBPL",
        1864u16 => "application/vnd.fujixerox.ddd",
        1865u16 => "application/vnd.fujixerox.docuworks",
        1866u16 => "application/vnd.fujixerox.docuworks.binder",
        1867u16 => "application/vnd.fujixerox.docuworks.container",
        1868u16 => "application/vnd.fut-misnet",
        1869u16 => "application/vnd.futoin+cbor",
        1870u16 => "application/vnd.futoin+json",
        1871u16 => "application/vnd.fuzzysheet",
        1872u16 => "application/vnd.genomatix.tuxedo",
        1873u16 => "application/vnd.genozip",
        1874u16 => "application/vnd.gentics.grd+json",
        1875u16 => "application/vnd.gentoo.catmetadata+xml",
        1876u16 => "application/vnd.gentoo.ebuild",
        1877u16 => "application/vnd.gentoo.eclass",
        1878u16 => "application/vnd.gentoo.gpkg",
        1879u16 => "application/vnd.gentoo.manifest",
        1880u16 => "application/vnd.gentoo.pkgmetadata+xml",
        1881u16 => "application/vnd.gentoo.xpak",
        1882u16 => "application/vnd.geo+json",
        1883u16 => "application/vnd.geocube+xml",
        1884u16 => "application/vnd.geogebra.file",
        1885u16 => "application/vnd.geogebra.slides",
        1886u16 => "application/vnd.geogebra.tool",
        1887u16 => "application/vnd.geometry-explorer",
        1888u16 => "application/vnd.geonext",
        1889u16 => "application/vnd.geoplan",
        1890u16 => "application/vnd.geospace",
        1891u16 => "application/vnd.gerber",
        1892u16 => "application/vnd.globalplatform.card-content-mgt",
        1893u16 => "application/vnd.globalplatform.card-content-mgt-response",
        1894u16 => "application/vnd.gmx",
        1895u16 => "application/vnd.gnu.taler.exchange+json",
        1896u16 => "application/vnd.gnu.taler.merchant+json",
        1897u16 => "application/vnd.google-earth.kml+xml",
        1898u16 => "application/vnd.google-earth.kmz",
        1899u16 => "application/vnd.gov.sk.e-form+xml",
        1900u16 => "application/vnd.gov.sk.e-form+zip",
        1901u16 => "application/vnd.gov.sk.xmldatacontainer+xml",
        1902u16 => "application/vnd.gpxsee.map+xml",
        1903u16 => "application/vnd.grafeq",
        1904u16 => "application/vnd.gridmp",
        1905u16 => "application/vnd.groove-account",
        1906u16 => "application/vnd.groove-help",
        1907u16 => "application/vnd.groove-identity-message",
        1908u16 => "application/vnd.groove-injector",
        1909u16 => "application/vnd.groove-tool-message",
        1910u16 => "application/vnd.groove-tool-template",
        1911u16 => "application/vnd.groove-vcard",
        1912u16 => "application/vnd.hal+json",
        1913u16 => "application/vnd.hal+xml",
        1914u16 => "application/vnd.hbci",
        1915u16 => "application/vnd.hc+json",
        1916u16 => "application/vnd.hcl-bireports",
        1917u16 => "application/vnd.hdt",
        1918u16 => "application/vnd.heroku+json",
        1919u16 => "application/vnd.hhe.lesson-player",
        1920u16 => "application/vnd.hp-HPGL",
        1921u16 => "application/vnd.hp-PCL",
        1922u16 => "application/vnd.hp-PCLXL",
        1923u16 => "application/vnd.hp-hpid",
        1924u16 => "application/vnd.hp-hps",
        1925u16 => "application/vnd.hp-jlyt",
        1926u16 => "application/vnd.hsl",
        1927u16 => "application/vnd.httphone",
        1928u16 => "application/vnd.hydrostatix.sof-data",
        1929u16 => "application/vnd.hyper+json",
        1930u16 => "application/vnd.hyper-item+json",
        1931u16 => "application/vnd.hyperdrive+json",
        1932u16 => "application/vnd.hzn-3d-crossword",
        1933u16 => "application/vnd.ibm.MiniPay",
        1934u16 => "application/vnd.ibm.afplinedata",
        1935u16 => "application/vnd.ibm.electronic-media",
        1936u16 => "application/vnd.ibm.modcap",
        1937u16 => "application/vnd.ibm.rights-management",
        1938u16 => "application/vnd.ibm.secure-container",
        1939u16 => "application/vnd.iccprofile",
        1940u16 => "application/vnd.ieee.1905",
        1941u16 => "application/vnd.igloader",
        1942u16 => "application/vnd.imagemeter.folder+zip",
        1943u16 => "application/vnd.imagemeter.image+zip",
        1944u16 => "application/vnd.immervision-ivp",
        1945u16 => "application/vnd.immervision-ivu",
        1946u16 => "application/vnd.ims.imsccv1p1",
        1947u16 => "application/vnd.ims.imsccv1p2",
        1948u16 => "application/vnd.ims.imsccv1p3",
        1949u16 => "application/vnd.ims.lis.v2.result+json",
        1950u16 => "application/vnd.ims.lti.v2.toolconsumerprofile+json",
        1951u16 => "application/vnd.ims.lti.v2.toolproxy+json",
        1952u16 => "application/vnd.ims.lti.v2.toolproxy.id+json",
        1953u16 => "application/vnd.ims.lti.v2.toolsettings+json",
        1954u16 => "application/vnd.ims.lti.v2.toolsettings.simple+json",
        1955u16 => "application/vnd.informedcontrol.rms+xml",
        1956u16 => "application/vnd.informix-visionary",
        1957u16 => "application/vnd.infotech.project",
        1958u16 => "application/vnd.infotech.project+xml",
        1959u16 => "application/vnd.innopath.wamp.notification",
        1960u16 => "application/vnd.insors.igm",
        1961u16 => "application/vnd.intercon.formnet",
        1962u16 => "application/vnd.intergeo",
        1963u16 => "application/vnd.intertrust.digibox",
        1964u16 => "application/vnd.intertrust.nncp",
        1965u16 => "application/vnd.intu.qbo",
        1966u16 => "application/vnd.intu.qfx",
        1967u16 => "application/vnd.ipfs.ipns-record",
        1968u16 => "application/vnd.ipld.car",
        1969u16 => "application/vnd.ipld.dag-cbor",
        1970u16 => "application/vnd.ipld.dag-json",
        1971u16 => "application/vnd.ipld.raw",
        1972u16 => "application/vnd.iptc.g2.catalogitem+xml",
        1973u16 => "application/vnd.iptc.g2.conceptitem+xml",
        1974u16 => "application/vnd.iptc.g2.knowledgeitem+xml",
        1975u16 => "application/vnd.iptc.g2.newsitem+xml",
        1976u16 => "application/vnd.iptc.g2.newsmessage+xml",
        1977u16 => "application/vnd.iptc.g2.packageitem+xml",
        1978u16 => "application/vnd.iptc.g2.planningitem+xml",
        1979u16 => "application/vnd.ipunplugged.rcprofile",
        1980u16 => "application/vnd.irepository.package+xml",
        1981u16 => "application/vnd.is-xpr",
        1982u16 => "application/vnd.isac.fcs",
        1983u16 => "application/vnd.iso11783-10+zip",
        1984u16 => "application/vnd.jam",
        1985u16 => "application/vnd.japannet-directory-service",
        1986u16 => "application/vnd.japannet-jpnstore-wakeup",
        1987u16 => "application/vnd.japannet-payment-wakeup",
        1988u16 => "application/vnd.japannet-registration",
        1989u16 => "application/vnd.japannet-registration-wakeup",
        1990u16 => "application/vnd.japannet-setstore-wakeup",
        1991u16 => "application/vnd.japannet-verification",
        1992u16 => "application/vnd.japannet-verification-wakeup",
        1993u16 => "application/vnd.jcp.javame.midlet-rms",
        1994u16 => "application/vnd.jisp",
        1995u16 => "application/vnd.joost.joda-archive",
        1996u16 => "application/vnd.jsk.isdn-ngn",
        1997u16 => "application/vnd.kahootz",
        1998u16 => "application/vnd.kde.karbon",
        1999u16 => "application/vnd.kde.kchart",
        2000u16 => "application/vnd.kde.kformula",
        2001u16 => "application/vnd.kde.kivio",
        2002u16 => "application/vnd.kde.kontour",
        2003u16 => "application/vnd.kde.kpresenter",
        2004u16 => "application/vnd.kde.kspread",
        2005u16 => "application/vnd.kde.kword",
        2006u16 => "application/vnd.kenameaapp",
        2007u16 => "application/vnd.kidspiration",
        2008u16 => "application/vnd.koan",
        2009u16 => "application/vnd.kodak-descriptor",
        2010u16 => "application/vnd.las",
        2011u16 => "application/vnd.las.las+json",
        2012u16 => "application/vnd.las.las+xml",
        2013u16 => "application/vnd.laszip",
        2014u16 => "application/vnd.ldev.productlicensing",
        2015u16 => "application/vnd.leap+json",
        2016u16 => "application/vnd.liberty-request+xml",
        2017u16 => "application/vnd.llamagraphics.life-balance.desktop",
        2018u16 => "application/vnd.llamagraphics.life-balance.exchange+xml",
        2019u16 => "application/vnd.logipipe.circuit+zip",
        2020u16 => "application/vnd.loom",
        2021u16 => "application/vnd.lotus-1-2-3",
        2022u16 => "application/vnd.lotus-approach",
        2023u16 => "application/vnd.lotus-freelance",
        2024u16 => "application/vnd.lotus-notes",
        2025u16 => "application/vnd.lotus-organizer",
        2026u16 => "application/vnd.lotus-screencam",
        2027u16 => "application/vnd.lotus-wordpro",
        2028u16 => "application/vnd.macports.portpkg",
        2029u16 => "application/vnd.mapbox-vector-tile",
        2030u16 => "application/vnd.marlin.drm.actiontoken+xml",
        2031u16 => "application/vnd.marlin.drm.conftoken+xml",
        2032u16 => "application/vnd.marlin.drm.license+xml",
        2033u16 => "application/vnd.marlin.drm.mdcf",
        2034u16 => "application/vnd.mason+json",
        2035u16 => "application/vnd.maxar.archive.3tz+zip",
        2036u16 => "application/vnd.maxmind.maxmind-db",
        2037u16 => "application/vnd.mcd",
        2038u16 => "application/vnd.mdl",
        2039u16 => "application/vnd.mdl-mbsdf",
        2040u16 => "application/vnd.medcalcdata",
        2041u16 => "application/vnd.mediastation.cdkey",
        2042u16 => "application/vnd.medicalholodeck.recordxr",
        2043u16 => "application/vnd.meridian-slingshot",
        2044u16 => "application/vnd.mermaid",
        2045u16 => "application/vnd.mfmp",
        2046u16 => "application/vnd.micro+json",
        2047u16 => "application/vnd.micrografx.flo",
        2048u16 => "application/vnd.micrografx.igx",
        2049u16 => "application/vnd.microsoft.portable-executable",
        2050u16 => "application/vnd.microsoft.windows.thumbnail-cache",
        2051u16 => "application/vnd.miele+json",
        2052u16 => "application/vnd.mif",
        2053u16 => "application/vnd.minisoft-hp3000-save",
        2054u16 => "application/vnd.mitsubishi.misty-guard.trustweb",
        2055u16 => "application/vnd.modl",
        2056u16 => "application/vnd.mophun.application",
        2057u16 => "application/vnd.mophun.certificate",
        2058u16 => "application/vnd.motorola.flexsuite",
        2059u16 => "application/vnd.motorola.flexsuite.adsi",
        2060u16 => "application/vnd.motorola.flexsuite.fis",
        2061u16 => "application/vnd.motorola.flexsuite.gotap",
        2062u16 => "application/vnd.motorola.flexsuite.kmr",
        2063u16 => "application/vnd.motorola.flexsuite.ttc",
        2064u16 => "application/vnd.motorola.flexsuite.wem",
        2065u16 => "application/vnd.motorola.iprm",
        2066u16 => "application/vnd.mozilla.xul+xml",
        2067u16 => "application/vnd.ms-3mfdocument",
        2068u16 => "application/vnd.ms-PrintDeviceCapabilities+xml",
        2069u16 => "application/vnd.ms-PrintSchemaTicket+xml",
        2070u16 => "application/vnd.ms-artgalry",
        2071u16 => "application/vnd.ms-asf",
        2072u16 => "application/vnd.ms-cab-compressed",
        2073u16 => "application/vnd.ms-excel",
        2074u16 => "application/vnd.ms-excel.addin.macroEnabled.12",
        2075u16 => "application/vnd.ms-excel.sheet.binary.macroEnabled.12",
        2076u16 => "application/vnd.ms-excel.sheet.macroEnabled.12",
        2077u16 => "application/vnd.ms-excel.template.macroEnabled.12",
        2078u16 => "application/vnd.ms-fontobject",
        2079u16 => "application/vnd.ms-htmlhelp",
        2080u16 => "application/vnd.ms-ims",
        2081u16 => "application/vnd.ms-lrm",
        2082u16 => "application/vnd.ms-office.activeX+xml",
        2083u16 => "application/vnd.ms-officetheme",
        2084u16 => "application/vnd.ms-playready.initiator+xml",
        2085u16 => "application/vnd.ms-powerpoint",
        2086u16 => "application/vnd.ms-powerpoint.addin.macroEnabled.12",
        2087u16 => "application/vnd.ms-powerpoint.presentation.macroEnabled.12",
        2088u16 => "application/vnd.ms-powerpoint.slide.macroEnabled.12",
        2089u16 => "application/vnd.ms-powerpoint.slideshow.macroEnabled.12",
        2090u16 => "application/vnd.ms-powerpoint.template.macroEnabled.12",
        2091u16 => "application/vnd.ms-project",
        2092u16 => "application/vnd.ms-tnef",
        2093u16 => "application/vnd.ms-windows.devicepairing",
        2094u16 => "application/vnd.ms-windows.nwprinting.oob",
        2095u16 => "application/vnd.ms-windows.printerpairing",
        2096u16 => "application/vnd.ms-windows.wsd.oob",
        2097u16 => "application/vnd.ms-wmdrm.lic-chlg-req",
        2098u16 => "application/vnd.ms-wmdrm.lic-resp",
        2099u16 => "application/vnd.ms-wmdrm.meter-chlg-req",
        2100u16 => "application/vnd.ms-wmdrm.meter-resp",
        2101u16 => "application/vnd.ms-word.document.macroEnabled.12",
        2102u16 => "application/vnd.ms-word.template.macroEnabled.12",
        2103u16 => "application/vnd.ms-works",
        2104u16 => "application/vnd.ms-wpl",
        2105u16 => "application/vnd.ms-xpsdocument",
        2106u16 => "application/vnd.msa-disk-image",
        2107u16 => "application/vnd.mseq",
        2108u16 => "application/vnd.msign",
        2109u16 => "application/vnd.multiad.creator",
        2110u16 => "application/vnd.multiad.creator.cif",
        2111u16 => "application/vnd.music-niff",
        2112u16 => "application/vnd.musician",
        2113u16 => "application/vnd.muvee.style",
        2114u16 => "application/vnd.mynfc",
        2115u16 => "application/vnd.nacamar.ybrid+json",
        2116u16 => "application/vnd.nato.bindingdataobject+cbor",
        2117u16 => "application/vnd.nato.bindingdataobject+json",
        2118u16 => "application/vnd.nato.bindingdataobject+xml",
        2119u16 => "application/vnd.nato.openxmlformats-package.iepd+zip",
        2120u16 => "application/vnd.ncd.control",
        2121u16 => "application/vnd.ncd.reference",
        2122u16 => "application/vnd.nearst.inv+json",
        2123u16 => "application/vnd.nebumind.line",
        2124u16 => "application/vnd.nervana",
        2125u16 => "application/vnd.netfpx",
        2126u16 => "application/vnd.neurolanguage.nlu",
        2127u16 => "application/vnd.nimn",
        2128u16 => "application/vnd.nintendo.nitro.rom",
        2129u16 => "application/vnd.nintendo.snes.rom",
        2130u16 => "application/vnd.nitf",
        2131u16 => "application/vnd.noblenet-directory",
        2132u16 => "application/vnd.noblenet-sealer",
        2133u16 => "application/vnd.noblenet-web",
        2134u16 => "application/vnd.nokia.catalogs",
        2135u16 => "application/vnd.nokia.conml+wbxml",
        2136u16 => "application/vnd.nokia.conml+xml",
        2137u16 => "application/vnd.nokia.iSDS-radio-presets",
        2138u16 => "application/vnd.nokia.iptv.config+xml",
        2139u16 => "application/vnd.nokia.landmark+wbxml",
        2140u16 => "application/vnd.nokia.landmark+xml",
        2141u16 => "application/vnd.nokia.landmarkcollection+xml",
        2142u16 => "application/vnd.nokia.n-gage.ac+xml",
        2143u16 => "application/vnd.nokia.n-gage.data",
        2144u16 => "application/vnd.nokia.n-gage.symbian.install",
        2145u16 => "application/vnd.nokia.ncd",
        2146u16 => "application/vnd.nokia.pcd+wbxml",
        2147u16 => "application/vnd.nokia.pcd+xml",
        2148u16 => "application/vnd.nokia.radio-preset",
        2149u16 => "application/vnd.nokia.radio-presets",
        2150u16 => "application/vnd.novadigm.EDM",
        2151u16 => "application/vnd.novadigm.EDX",
        2152u16 => "application/vnd.novadigm.EXT",
        2153u16 => "application/vnd.ntt-local.content-share",
        2154u16 => "application/vnd.ntt-local.file-transfer",
        2155u16 => "application/vnd.ntt-local.ogw_remote-access",
        2156u16 => "application/vnd.ntt-local.sip-ta_remote",
        2157u16 => "application/vnd.ntt-local.sip-ta_tcp_stream",
        2158u16 => "application/vnd.oai.workflows",
        2159u16 => "application/vnd.oai.workflows+json",
        2160u16 => "application/vnd.oai.workflows+yaml",
        2161u16 => "application/vnd.oasis.opendocument.base",
        2162u16 => "application/vnd.oasis.opendocument.chart",
        2163u16 => "application/vnd.oasis.opendocument.chart-template",
        2164u16 => "application/vnd.oasis.opendocument.database",
        2165u16 => "application/vnd.oasis.opendocument.formula",
        2166u16 => "application/vnd.oasis.opendocument.formula-template",
        2167u16 => "application/vnd.oasis.opendocument.graphics",
        2168u16 => "application/vnd.oasis.opendocument.graphics-template",
        2169u16 => "application/vnd.oasis.opendocument.image",
        2170u16 => "application/vnd.oasis.opendocument.image-template",
        2171u16 => "application/vnd.oasis.opendocument.presentation",
        2172u16 => "application/vnd.oasis.opendocument.presentation-template",
        2173u16 => "application/vnd.oasis.opendocument.spreadsheet",
        2174u16 => "application/vnd.oasis.opendocument.spreadsheet-template",
        2175u16 => "application/vnd.oasis.opendocument.text",
        2176u16 => "application/vnd.oasis.opendocument.text-master",
        2177u16 => "application/vnd.oasis.opendocument.text-master-template",
        2178u16 => "application/vnd.oasis.opendocument.text-template",
        2179u16 => "application/vnd.oasis.opendocument.text-web",
        2180u16 => "application/vnd.obn",
        2181u16 => "application/vnd.ocf+cbor",
        2182u16 => "application/vnd.oci.image.manifest.v1+json",
        2183u16 => "application/vnd.oftn.l10n+json",
        2184u16 => "application/vnd.oipf.contentaccessdownload+xml",
        2185u16 => "application/vnd.oipf.contentaccessstreaming+xml",
        2186u16 => "application/vnd.oipf.cspg-hexbinary",
        2187u16 => "application/vnd.oipf.dae.svg+xml",
        2188u16 => "application/vnd.oipf.dae.xhtml+xml",
        2189u16 => "application/vnd.oipf.mippvcontrolmessage+xml",
        2190u16 => "application/vnd.oipf.pae.gem",
        2191u16 => "application/vnd.oipf.spdiscovery+xml",
        2192u16 => "application/vnd.oipf.spdlist+xml",
        2193u16 => "application/vnd.oipf.ueprofile+xml",
        2194u16 => "application/vnd.oipf.userprofile+xml",
        2195u16 => "application/vnd.olpc-sugar",
        2196u16 => "application/vnd.oma-scws-config",
        2197u16 => "application/vnd.oma-scws-http-request",
        2198u16 => "application/vnd.oma-scws-http-response",
        2199u16 => "application/vnd.oma.bcast.associated-procedure-parameter+xml",
        2200u16 => "application/vnd.oma.bcast.drm-trigger+xml",
        2201u16 => "application/vnd.oma.bcast.imd+xml",
        2202u16 => "application/vnd.oma.bcast.ltkm",
        2203u16 => "application/vnd.oma.bcast.notification+xml",
        2204u16 => "application/vnd.oma.bcast.provisioningtrigger",
        2205u16 => "application/vnd.oma.bcast.sgboot",
        2206u16 => "application/vnd.oma.bcast.sgdd+xml",
        2207u16 => "application/vnd.oma.bcast.sgdu",
        2208u16 => "application/vnd.oma.bcast.simple-symbol-container",
        2209u16 => "application/vnd.oma.bcast.smartcard-trigger+xml",
        2210u16 => "application/vnd.oma.bcast.sprov+xml",
        2211u16 => "application/vnd.oma.bcast.stkm",
        2212u16 => "application/vnd.oma.cab-address-book+xml",
        2213u16 => "application/vnd.oma.cab-feature-handler+xml",
        2214u16 => "application/vnd.oma.cab-pcc+xml",
        2215u16 => "application/vnd.oma.cab-subs-invite+xml",
        2216u16 => "application/vnd.oma.cab-user-prefs+xml",
        2217u16 => "application/vnd.oma.dcd",
        2218u16 => "application/vnd.oma.dcdc",
        2219u16 => "application/vnd.oma.dd2+xml",
        2220u16 => "application/vnd.oma.drm.risd+xml",
        2221u16 => "application/vnd.oma.group-usage-list+xml",
        2222u16 => "application/vnd.oma.lwm2m+cbor",
        2223u16 => "application/vnd.oma.lwm2m+json",
        2224u16 => "application/vnd.oma.lwm2m+tlv",
        2225u16 => "application/vnd.oma.pal+xml",
        2226u16 => "application/vnd.oma.poc.detailed-progress-report+xml",
        2227u16 => "application/vnd.oma.poc.final-report+xml",
        2228u16 => "application/vnd.oma.poc.groups+xml",
        2229u16 => "application/vnd.oma.poc.invocation-descriptor+xml",
        2230u16 => "application/vnd.oma.poc.optimized-progress-report+xml",
        2231u16 => "application/vnd.oma.push",
        2232u16 => "application/vnd.oma.scidm.messages+xml",
        2233u16 => "application/vnd.oma.xcap-directory+xml",
        2234u16 => "application/vnd.omads-email+xml",
        2235u16 => "application/vnd.omads-file+xml",
        2236u16 => "application/vnd.omads-folder+xml",
        2237u16 => "application/vnd.omaloc-supl-init",
        2238u16 => "application/vnd.onepager",
        2239u16 => "application/vnd.onepagertamp",
        2240u16 => "application/vnd.onepagertamx",
        2241u16 => "application/vnd.onepagertat",
        2242u16 => "application/vnd.onepagertatp",
        2243u16 => "application/vnd.onepagertatx",
        2244u16 => "application/vnd.onvif.metadata",
        2245u16 => "application/vnd.openblox.game+xml",
        2246u16 => "application/vnd.openblox.game-binary",
        2247u16 => "application/vnd.openeye.oeb",
        2248u16 => "application/vnd.openstreetmap.data+xml",
        2249u16 => "application/vnd.opentimestamps.ots",
        2250u16 => "application/vnd.openxmlformats-officedocument.custom-properties+xml",
        2251u16 => "application/vnd.openxmlformats-officedocument.customXmlProperties+xml",
        2252u16 => "application/vnd.openxmlformats-officedocument.drawing+xml",
        2253u16 => "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
        2254u16 => "application/vnd.openxmlformats-officedocument.drawingml.chartshapes+xml",
        2255u16 => "application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml",
        2256u16 => "application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml",
        2257u16 => "application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml",
        2258u16 => "application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml",
        2259u16 => "application/vnd.openxmlformats-officedocument.extended-properties+xml",
        2260u16 => "application/vnd.openxmlformats-officedocument.presentationml.commentAuthors+xml",
        2261u16 => "application/vnd.openxmlformats-officedocument.presentationml.comments+xml",
        2262u16 => "application/vnd.openxmlformats-officedocument.presentationml.handoutMaster+xml",
        2263u16 => "application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml",
        2264u16 => "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml",
        2265u16 => "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml",
        2266u16 => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        2267u16 => "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
        2268u16 => "application/vnd.openxmlformats-officedocument.presentationml.slide",
        2269u16 => "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
        2270u16 => "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml",
        2271u16 => "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml",
        2272u16 => "application/vnd.openxmlformats-officedocument.presentationml.slideUpdateInfo+xml",
        2273u16 => "application/vnd.openxmlformats-officedocument.presentationml.slideshow",
        2274u16 => "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml",
        2275u16 => "application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml",
        2276u16 => "application/vnd.openxmlformats-officedocument.presentationml.tags+xml",
        2277u16 => "application/vnd.openxmlformats-officedocument.presentationml.template",
        2278u16 => "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml",
        2279u16 => "application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml",
        2280u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml",
        2281u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml",
        2282u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml",
        2283u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml",
        2284u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.dialogsheet+xml",
        2285u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml",
        2286u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml",
        2287u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml",
        2288u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml",
        2289u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.queryTable+xml",
        2290u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionHeaders+xml",
        2291u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionLog+xml",
        2292u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml",
        2293u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        2294u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        2295u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml",
        2296u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
        2297u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml",
        2298u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.tableSingleCells+xml",
        2299u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.template",
        2300u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml",
        2301u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.userNames+xml",
        2302u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.volatileDependencies+xml",
        2303u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
        2304u16 => "application/vnd.openxmlformats-officedocument.theme+xml",
        2305u16 => "application/vnd.openxmlformats-officedocument.themeOverride+xml",
        2306u16 => "application/vnd.openxmlformats-officedocument.vmlDrawing",
        2307u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        2308u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        2309u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.document.glossary+xml",
        2310u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        2311u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml",
        2312u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml",
        2313u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
        2314u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
        2315u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
        2316u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml",
        2317u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
        2318u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.template",
        2319u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml",
        2320u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.webSettings+xml",
        2321u16 => "application/vnd.openxmlformats-package.core-properties+xml",
        2322u16 => "application/vnd.openxmlformats-package.digital-signature-xmlsignature+xml",
        2323u16 => "application/vnd.openxmlformats-package.relationships+xml",
        2324u16 => "application/vnd.oracle.resource+json",
        2325u16 => "application/vnd.orange.indata",
        2326u16 => "application/vnd.osa.netdeploy",
        2327u16 => "application/vnd.osgeo.mapguide.package",
        2328u16 => "application/vnd.osgi.bundle",
        2329u16 => "application/vnd.osgi.dp",
        2330u16 => "application/vnd.osgi.subsystem",
        2331u16 => "application/vnd.otps.ct-kip+xml",
        2332u16 => "application/vnd.oxli.countgraph",
        2333u16 => "application/vnd.pagerduty+json",
        2334u16 => "application/vnd.palm",
        2335u16 => "application/vnd.panoply",
        2336u16 => "application/vnd.paos.xml",
        2337u16 => "application/vnd.patentdive",
        2338u16 => "application/vnd.patientecommsdoc",
        2339u16 => "application/vnd.pawaafile",
        2340u16 => "application/vnd.pcos",
        2341u16 => "application/vnd.pg.format",
        2342u16 => "application/vnd.pg.osasli",
        2343u16 => "application/vnd.piaccess.application-licence",
        2344u16 => "application/vnd.picsel",
        2345u16 => "application/vnd.pmi.widget",
        2346u16 => "application/vnd.poc.group-advertisement+xml",
        2347u16 => "application/vnd.pocketlearn",
        2348u16 => "application/vnd.powerbuilder6",
        2349u16 => "application/vnd.powerbuilder6-s",
        2350u16 => "application/vnd.powerbuilder7",
        2351u16 => "application/vnd.powerbuilder7-s",
        2352u16 => "application/vnd.powerbuilder75",
        2353u16 => "application/vnd.powerbuilder75-s",
        2354u16 => "application/vnd.preminet",
        2355u16 => "application/vnd.previewsystems.box",
        2356u16 => "application/vnd.proteus.magazine",
        2357u16 => "application/vnd.psfs",
        2358u16 => "application/vnd.pt.mundusmundi",
        2359u16 => "application/vnd.publishare-delta-tree",
        2360u16 => "application/vnd.pvi.ptid1",
        2361u16 => "application/vnd.pwg-multiplexed",
        2362u16 => "application/vnd.pwg-xhtml-print+xml",
        2363u16 => "application/vnd.qualcomm.brew-app-res",
        2364u16 => "application/vnd.quarantainenet",
        2365u16 => "application/vnd.quobject-quoxdocument",
        2366u16 => "application/vnd.radisys.moml+xml",
        2367u16 => "application/vnd.radisys.msml+xml",
        2368u16 => "application/vnd.radisys.msml-audit+xml",
        2369u16 => "application/vnd.radisys.msml-audit-conf+xml",
        2370u16 => "application/vnd.radisys.msml-audit-conn+xml",
        2371u16 => "application/vnd.radisys.msml-audit-dialog+xml",
        2372u16 => "application/vnd.radisys.msml-audit-stream+xml",
        2373u16 => "application/vnd.radisys.msml-conf+xml",
        2374u16 => "application/vnd.radisys.msml-dialog+xml",
        2375u16 => "application/vnd.radisys.msml-dialog-base+xml",
        2376u16 => "application/vnd.radisys.msml-dialog-fax-detect+xml",
        2377u16 => "application/vnd.radisys.msml-dialog-fax-sendrecv+xml",
        2378u16 => "application/vnd.radisys.msml-dialog-group+xml",
        2379u16 => "application/vnd.radisys.msml-dialog-speech+xml",
        2380u16 => "application/vnd.radisys.msml-dialog-transform+xml",
        2381u16 => "application/vnd.rainstor.data",
        2382u16 => "application/vnd.rapid",
        2383u16 => "application/vnd.rar",
        2384u16 => "application/vnd.realvnc.bed",
        2385u16 => "application/vnd.recordare.musicxml",
        2386u16 => "application/vnd.recordare.musicxml+xml",
        2387u16 => "application/vnd.relpipe",
        2388u16 => "application/vnd.resilient.logic",
        2389u16 => "application/vnd.restful+json",
        2390u16 => "application/vnd.rig.cryptonote",
        2391u16 => "application/vnd.route66.link66+xml",
        2392u16 => "application/vnd.rs-274x",
        2393u16 => "application/vnd.ruckus.download",
        2394u16 => "application/vnd.s3sms",
        2395u16 => "application/vnd.sailingtracker.track",
        2396u16 => "application/vnd.sar",
        2397u16 => "application/vnd.sbm.cid",
        2398u16 => "application/vnd.sbm.mid2",
        2399u16 => "application/vnd.scribus",
        2400u16 => "application/vnd.sealed.3df",
        2401u16 => "application/vnd.sealed.csf",
        2402u16 => "application/vnd.sealed.doc",
        2403u16 => "application/vnd.sealed.eml",
        2404u16 => "application/vnd.sealed.mht",
        2405u16 => "application/vnd.sealed.net",
        2406u16 => "application/vnd.sealed.ppt",
        2407u16 => "application/vnd.sealed.tiff",
        2408u16 => "application/vnd.sealed.xls",
        2409u16 => "application/vnd.sealedmedia.softseal.html",
        2410u16 => "application/vnd.sealedmedia.softseal.pdf",
        2411u16 => "application/vnd.seemail",
        2412u16 => "application/vnd.seis+json",
        2413u16 => "application/vnd.sema",
        2414u16 => "application/vnd.semd",
        2415u16 => "application/vnd.semf",
        2416u16 => "application/vnd.shade-save-file",
        2417u16 => "application/vnd.shana.informed.formdata",
        2418u16 => "application/vnd.shana.informed.formtemplate",
        2419u16 => "application/vnd.shana.informed.interchange",
        2420u16 => "application/vnd.shana.informed.package",
        2421u16 => "application/vnd.shootproof+json",
        2422u16 => "application/vnd.shopkick+json",
        2423u16 => "application/vnd.shp",
        2424u16 => "application/vnd.shx",
        2425u16 => "application/vnd.sigrok.session",
        2426u16 => "application/vnd.siren+json",
        2427u16 => "application/vnd.smaf",
        2428u16 => "application/vnd.smart.notebook",
        2429u16 => "application/vnd.smart.teacher",
        2430u16 => "application/vnd.smintio.portals.archive",
        2431u16 => "application/vnd.snesdev-page-table",
        2432u16 => "application/vnd.software602.filler.form+xml",
        2433u16 => "application/vnd.software602.filler.form-xml-zip",
        2434u16 => "application/vnd.solent.sdkm+xml",
        2435u16 => "application/vnd.spotfire.dxp",
        2436u16 => "application/vnd.spotfire.sfs",
        2437u16 => "application/vnd.sqlite3",
        2438u16 => "application/vnd.sss-cod",
        2439u16 => "application/vnd.sss-dtf",
        2440u16 => "application/vnd.sss-ntf",
        2441u16 => "application/vnd.stepmania.package",
        2442u16 => "application/vnd.stepmania.stepchart",
        2443u16 => "application/vnd.street-stream",
        2444u16 => "application/vnd.sun.wadl+xml",
        2445u16 => "application/vnd.sus-calendar",
        2446u16 => "application/vnd.svd",
        2447u16 => "application/vnd.swiftview-ics",
        2448u16 => "application/vnd.sybyl.mol2",
        2449u16 => "application/vnd.sycle+xml",
        2450u16 => "application/vnd.syft+json",
        2451u16 => "application/vnd.syncml+xml",
        2452u16 => "application/vnd.syncml.dm+wbxml",
        2453u16 => "application/vnd.syncml.dm+xml",
        2454u16 => "application/vnd.syncml.dm.notification",
        2455u16 => "application/vnd.syncml.dmddf+wbxml",
        2456u16 => "application/vnd.syncml.dmddf+xml",
        2457u16 => "application/vnd.syncml.dmtnds+wbxml",
        2458u16 => "application/vnd.syncml.dmtnds+xml",
        2459u16 => "application/vnd.syncml.ds.notification",
        2460u16 => "application/vnd.tableschema+json",
        2461u16 => "application/vnd.tao.intent-module-archive",
        2462u16 => "application/vnd.tcpdump.pcap",
        2463u16 => "application/vnd.think-cell.ppttc+json",
        2464u16 => "application/vnd.tmd.mediaflex.api+xml",
        2465u16 => "application/vnd.tml",
        2466u16 => "application/vnd.tmobile-livetv",
        2467u16 => "application/vnd.tri.onesource",
        2468u16 => "application/vnd.trid.tpt",
        2469u16 => "application/vnd.triscape.mxs",
        2470u16 => "application/vnd.trueapp",
        2471u16 => "application/vnd.truedoc",
        2472u16 => "application/vnd.ubisoft.webplayer",
        2473u16 => "application/vnd.ufdl",
        2474u16 => "application/vnd.uiq.theme",
        2475u16 => "application/vnd.umajin",
        2476u16 => "application/vnd.unity",
        2477u16 => "application/vnd.uoml+xml",
        2478u16 => "application/vnd.uplanet.alert",
        2479u16 => "application/vnd.uplanet.alert-wbxml",
        2480u16 => "application/vnd.uplanet.bearer-choice",
        2481u16 => "application/vnd.uplanet.bearer-choice-wbxml",
        2482u16 => "application/vnd.uplanet.cacheop",
        2483u16 => "application/vnd.uplanet.cacheop-wbxml",
        2484u16 => "application/vnd.uplanet.channel",
        2485u16 => "application/vnd.uplanet.channel-wbxml",
        2486u16 => "application/vnd.uplanet.list",
        2487u16 => "application/vnd.uplanet.list-wbxml",
        2488u16 => "application/vnd.uplanet.listcmd",
        2489u16 => "application/vnd.uplanet.listcmd-wbxml",
        2490u16 => "application/vnd.uplanet.signal",
        2491u16 => "application/vnd.uri-map",
        2492u16 => "application/vnd.valve.source.material",
        2493u16 => "application/vnd.vcx",
        2494u16 => "application/vnd.vd-study",
        2495u16 => "application/vnd.vectorworks",
        2496u16 => "application/vnd.vel+json",
        2497u16 => "application/vnd.verimatrix.vcas",
        2498u16 => "application/vnd.veritone.aion+json",
        2499u16 => "application/vnd.veryant.thin",
        2500u16 => "application/vnd.ves.encrypted",
        2501u16 => "application/vnd.vidsoft.vidconference",
        2502u16 => "application/vnd.visio",
        2503u16 => "application/vnd.visionary",
        2504u16 => "application/vnd.vividence.scriptfile",
        2505u16 => "application/vnd.vsf",
        2506u16 => "application/vnd.wap.sic",
        2507u16 => "application/vnd.wap.slc",
        2508u16 => "application/vnd.wap.wbxml",
        2509u16 => "application/vnd.wap.wmlc",
        2510u16 => "application/vnd.wap.wmlscriptc",
        2511u16 => "application/vnd.wasmflow.wafl",
        2512u16 => "application/vnd.webturbo",
        2513u16 => "application/vnd.wfa.dpp",
        2514u16 => "application/vnd.wfa.p2p",
        2515u16 => "application/vnd.wfa.wsc",
        2516u16 => "application/vnd.windows.devicepairing",
        2517u16 => "application/vnd.wmc",
        2518u16 => "application/vnd.wmf.bootstrap",
        2519u16 => "application/vnd.wolfram.mathematica",
        2520u16 => "application/vnd.wolfram.mathematica.package",
        2521u16 => "application/vnd.wolfram.player",
        2522u16 => "application/vnd.wordlift",
        2523u16 => "application/vnd.wordperfect",
        2524u16 => "application/vnd.wqd",
        2525u16 => "application/vnd.wrq-hp3000-labelled",
        2526u16 => "application/vnd.wt.stf",
        2527u16 => "application/vnd.wv.csp+wbxml",
        2528u16 => "application/vnd.wv.csp+xml",
        2529u16 => "application/vnd.wv.ssp+xml",
        2530u16 => "application/vnd.xacml+json",
        2531u16 => "application/vnd.xara",
        2532u16 => "application/vnd.xecrets-encrypted",
        2533u16 => "application/vnd.xfdl",
        2534u16 => "application/vnd.xfdl.webform",
        2535u16 => "application/vnd.xmi+xml",
        2536u16 => "application/vnd.xmpie.cpkg",
        2537u16 => "application/vnd.xmpie.dpkg",
        2538u16 => "application/vnd.xmpie.plan",
        2539u16 => "application/vnd.xmpie.ppkg",
        2540u16 => "application/vnd.xmpie.xlim",
        2541u16 => "application/vnd.yamaha.hv-dic",
        2542u16 => "application/vnd.yamaha.hv-script",
        2543u16 => "application/vnd.yamaha.hv-voice",
        2544u16 => "application/vnd.yamaha.openscoreformat",
        2545u16 => "application/vnd.yamaha.openscoreformat.osfpvg+xml",
        2546u16 => "application/vnd.yamaha.remote-setup",
        2547u16 => "application/vnd.yamaha.smaf-audio",
        2548u16 => "application/vnd.yamaha.smaf-phrase",
        2549u16 => "application/vnd.yamaha.through-ngn",
        2550u16 => "application/vnd.yamaha.tunnel-udpencap",
        2551u16 => "application/vnd.yaoweme",
        2552u16 => "application/vnd.yellowriver-custom-menu",
        2553u16 => "application/vnd.youtube.yt",
        2554u16 => "application/vnd.zul",
        2555u16 => "application/vnd.zzazz.deck+xml",
        2556u16 => "application/voicexml+xml",
        2557u16 => "application/voucher-cms+json",
        2558u16 => "application/vq-rtcpxr",
        2559u16 => "application/wasm",
        2560u16 => "application/watcherinfo+xml",
        2561u16 => "application/webpush-options+json",
        2562u16 => "application/whoispp-query",
        2563u16 => "application/whoispp-response",
        2564u16 => "application/widget",
        2565u16 => "application/wita",
        2566u16 => "application/wordperfect5.1",
        2567u16 => "application/wsdl+xml",
        2568u16 => "application/wspolicy+xml",
        2569u16 => "application/x-pki-message",
        2570u16 => "application/x-www-form-urlencoded",
        2571u16 => "application/x-x509-ca-cert",
        2572u16 => "application/x-x509-ca-ra-cert",
        2573u16 => "application/x-x509-next-ca-cert",
        2574u16 => "application/x400-bp",
        2575u16 => "application/xacml+xml",
        2576u16 => "application/xcap-att+xml",
        2577u16 => "application/xcap-caps+xml",
        2578u16 => "application/xcap-diff+xml",
        2579u16 => "application/xcap-el+xml",
        2580u16 => "application/xcap-error+xml",
        2581u16 => "application/xcap-ns+xml",
        2582u16 => "application/xcon-conference-info+xml",
        2583u16 => "application/xcon-conference-info-diff+xml",
        2584u16 => "application/xenc+xml",
        2585u16 => "application/xfdf",
        2586u16 => "application/xhtml+xml",
        2587u16 => "application/xliff+xml",
        2588u16 => "application/xml",
        2589u16 => "application/xml-dtd",
        2590u16 => "application/xml-external-parsed-entity",
        2591u16 => "application/xml-patch+xml",
        2592u16 => "application/xmpp+xml",
        2593u16 => "application/xop+xml",
        2594u16 => "application/xslt+xml",
        2595u16 => "application/xv+xml",
        2596u16 => "application/yaml",
        2597u16 => "application/yang",
        2598u16 => "application/yang-data+cbor",
        2599u16 => "application/yang-data+json",
        2600u16 => "application/yang-data+xml",
        2601u16 => "application/yang-patch+json",
        2602u16 => "application/yang-patch+xml",
        2603u16 => "application/yang-sid+json",
        2604u16 => "application/yin+xml",
        2605u16 => "application/zip",
        2606u16 => "application/zlib",
        2607u16 => "application/zstd",
        2608u16 => "audio/1d-interleaved-parityfec",
        2609u16 => "audio/32kadpcm",
        2610u16 => "audio/3gpp",
        2611u16 => "audio/3gpp2",
        2612u16 => "audio/AMR",
        2613u16 => "audio/AMR-WB",
        2614u16 => "audio/ATRAC-ADVANCED-LOSSLESS",
        2615u16 => "audio/ATRAC-X",
        2616u16 => "audio/ATRAC3",
        2617u16 => "audio/BV16",
        2618u16 => "audio/BV32",
        2619u16 => "audio/CN",
        2620u16 => "audio/DAT12",
        2621u16 => "audio/DV",
        2622u16 => "audio/DVI4",
        2623u16 => "audio/EVRC",
        2624u16 => "audio/EVRC-QCP",
        2625u16 => "audio/EVRC0",
        2626u16 => "audio/EVRC1",
        2627u16 => "audio/EVRCB",
        2628u16 => "audio/EVRCB0",
        2629u16 => "audio/EVRCB1",
        2630u16 => "audio/EVRCNW",
        2631u16 => "audio/EVRCNW0",
        2632u16 => "audio/EVRCNW1",
        2633u16 => "audio/EVRCWB",
        2634u16 => "audio/EVRCWB0",
        2635u16 => "audio/EVRCWB1",
        2636u16 => "audio/EVS",
        2637u16 => "audio/G711-0",
        2638u16 => "audio/G719",
        2639u16 => "audio/G722",
        2640u16 => "audio/G7221",
        2641u16 => "audio/G723",
        2642u16 => "audio/G726-16",
        2643u16 => "audio/G726-24",
        2644u16 => "audio/G726-32",
        2645u16 => "audio/G726-40",
        2646u16 => "audio/G728",
        2647u16 => "audio/G729",
        2648u16 => "audio/G7291",
        2649u16 => "audio/G729D",
        2650u16 => "audio/G729E",
        2651u16 => "audio/GSM",
        2652u16 => "audio/GSM-EFR",
        2653u16 => "audio/GSM-HR-08",
        2654u16 => "audio/L16",
        2655u16 => "audio/L20",
        2656u16 => "audio/L24",
        2657u16 => "audio/L8",
        2658u16 => "audio/LPC",
        2659u16 => "audio/MELP",
        2660u16 => "audio/MELP1200",
        2661u16 => "audio/MELP2400",
        2662u16 => "audio/MELP600",
        2663u16 => "audio/MP4A-LATM",
        2664u16 => "audio/MPA",
        2665u16 => "audio/PCMA",
        2666u16 => "audio/PCMA-WB",
        2667u16 => "audio/PCMU",
        2668u16 => "audio/PCMU-WB",
        2669u16 => "audio/QCELP",
        2670u16 => "audio/RED",
        2671u16 => "audio/SMV",
        2672u16 => "audio/SMV-QCP",
        2673u16 => "audio/SMV0",
        2674u16 => "audio/TETRA_ACELP",
        2675u16 => "audio/TETRA_ACELP_BB",
        2676u16 => "audio/TSVCIS",
        2677u16 => "audio/UEMCLIP",
        2678u16 => "audio/VDVI",
        2679u16 => "audio/VMR-WB",
        2680u16 => "audio/aac",
        2681u16 => "audio/ac3",
        2682u16 => "audio/amr-wb+",
        2683u16 => "audio/aptx",
        2684u16 => "audio/asc",
        2685u16 => "audio/basic",
        2686u16 => "audio/clearmode",
        2687u16 => "audio/dls",
        2688u16 => "audio/dsr-es201108",
        2689u16 => "audio/dsr-es202050",
        2690u16 => "audio/dsr-es202211",
        2691u16 => "audio/dsr-es202212",
        2692u16 => "audio/eac3",
        2693u16 => "audio/encaprtp",
        2694u16 => "audio/example",
        2695u16 => "audio/flexfec",
        2696u16 => "audio/fwdred",
        2697u16 => "audio/iLBC",
        2698u16 => "audio/ip-mr_v2.5",
        2699u16 => "audio/matroska",
        2700u16 => "audio/mhas",
        2701u16 => "audio/mobile-xmf",
        2702u16 => "audio/mp4",
        2703u16 => "audio/mpa-robust",
        2704u16 => "audio/mpeg",
        2705u16 => "audio/mpeg4-generic",
        2706u16 => "audio/ogg",
        2707u16 => "audio/opus",
        2708u16 => "audio/parityfec",
        2709u16 => "audio/prs.sid",
        2710u16 => "audio/raptorfec",
        2711u16 => "audio/rtp-enc-aescm128",
        2712u16 => "audio/rtp-midi",
        2713u16 => "audio/rtploopback",
        2714u16 => "audio/rtx",
        2715u16 => "audio/scip",
        2716u16 => "audio/sofa",
        2717u16 => "audio/sp-midi",
        2718u16 => "audio/speex",
        2719u16 => "audio/t140c",
        2720u16 => "audio/t38",
        2721u16 => "audio/telephone-event",
        2722u16 => "audio/tone",
        2723u16 => "audio/ulpfec",
        2724u16 => "audio/usac",
        2725u16 => "audio/vnd.3gpp.iufp",
        2726u16 => "audio/vnd.4SB",
        2727u16 => "audio/vnd.CELP",
        2728u16 => "audio/vnd.audiokoz",
        2729u16 => "audio/vnd.cisco.nse",
        2730u16 => "audio/vnd.cmles.radio-events",
        2731u16 => "audio/vnd.cns.anp1",
        2732u16 => "audio/vnd.cns.inf1",
        2733u16 => "audio/vnd.dece.audio",
        2734u16 => "audio/vnd.digital-winds",
        2735u16 => "audio/vnd.dlna.adts",
        2736u16 => "audio/vnd.dolby.heaac.1",
        2737u16 => "audio/vnd.dolby.heaac.2",
        2738u16 => "audio/vnd.dolby.mlp",
        2739u16 => "audio/vnd.dolby.mps",
        2740u16 => "audio/vnd.dolby.pl2",
        2741u16 => "audio/vnd.dolby.pl2x",
        2742u16 => "audio/vnd.dolby.pl2z",
        2743u16 => "audio/vnd.dolby.pulse.1",
        2744u16 => "audio/vnd.dra",
        2745u16 => "audio/vnd.dts",
        2746u16 => "audio/vnd.dts.hd",
        2747u16 => "audio/vnd.dts.uhd",
        2748u16 => "audio/vnd.dvb.file",
        2749u16 => "audio/vnd.everad.plj",
        2750u16 => "audio/vnd.hns.audio",
        2751u16 => "audio/vnd.lucent.voice",
        2752u16 => "audio/vnd.ms-playready.media.pya",
        2753u16 => "audio/vnd.nokia.mobile-xmf",
        2754u16 => "audio/vnd.nortel.vbk",
        2755u16 => "audio/vnd.nuera.ecelp4800",
        2756u16 => "audio/vnd.nuera.ecelp7470",
        2757u16 => "audio/vnd.nuera.ecelp9600",
        2758u16 => "audio/vnd.octel.sbc",
        2759u16 => "audio/vnd.presonus.multitrack",
        2760u16 => "audio/vnd.qcelp",
        2761u16 => "audio/vnd.rhetorex.32kadpcm",
        2762u16 => "audio/vnd.rip",
        2763u16 => "audio/vnd.sealedmedia.softseal.mpeg",
        2764u16 => "audio/vnd.vmx.cvsd",
        2765u16 => "audio/vorbis",
        2766u16 => "audio/vorbis-config",
        2767u16 => "font/collection",
        2768u16 => "font/otf",
        2769u16 => "font/sfnt",
        2770u16 => "font/ttf",
        2771u16 => "font/woff",
        2772u16 => "font/woff2",
        2773u16 => "image/aces",
        2774u16 => "image/apng",
        2775u16 => "image/avci",
        2776u16 => "image/avcs",
        2777u16 => "image/avif",
        2778u16 => "image/bmp",
        2779u16 => "image/cgm",
        2780u16 => "image/dicom-rle",
        2781u16 => "image/dpx",
        2782u16 => "image/emf",
        2783u16 => "image/example",
        2784u16 => "image/fits",
        2785u16 => "image/g3fax",
        2786u16 => "image/gif",
        2787u16 => "image/heic",
        2788u16 => "image/heic-sequence",
        2789u16 => "image/heif",
        2790u16 => "image/heif-sequence",
        2791u16 => "image/hej2k",
        2792u16 => "image/hsj2",
        2793u16 => "image/ief",
        2794u16 => "image/j2c",
        2795u16 => "image/jls",
        2796u16 => "image/jp2",
        2797u16 => "image/jpeg",
        2798u16 => "image/jph",
        2799u16 => "image/jphc",
        2800u16 => "image/jpm",
        2801u16 => "image/jpx",
        2802u16 => "image/jxr",
        2803u16 => "image/jxrA",
        2804u16 => "image/jxrS",
        2805u16 => "image/jxs",
        2806u16 => "image/jxsc",
        2807u16 => "image/jxsi",
        2808u16 => "image/jxss",
        2809u16 => "image/ktx",
        2810u16 => "image/ktx2",
        2811u16 => "image/naplps",
        2812u16 => "image/png",
        2813u16 => "image/prs.btif",
        2814u16 => "image/prs.pti",
        2815u16 => "image/pwg-raster",
        2816u16 => "image/svg+xml",
        2817u16 => "image/t38",
        2818u16 => "image/tiff",
        2819u16 => "image/tiff-fx",
        2820u16 => "image/vnd.adobe.photoshop",
        2821u16 => "image/vnd.airzip.accelerator.azv",
        2822u16 => "image/vnd.cns.inf2",
        2823u16 => "image/vnd.dece.graphic",
        2824u16 => "image/vnd.djvu",
        2825u16 => "image/vnd.dvb.subtitle",
        2826u16 => "image/vnd.dwg",
        2827u16 => "image/vnd.dxf",
        2828u16 => "image/vnd.fastbidsheet",
        2829u16 => "image/vnd.fpx",
        2830u16 => "image/vnd.fst",
        2831u16 => "image/vnd.fujixerox.edmics-mmr",
        2832u16 => "image/vnd.fujixerox.edmics-rlc",
        2833u16 => "image/vnd.globalgraphics.pgb",
        2834u16 => "image/vnd.microsoft.icon",
        2835u16 => "image/vnd.mix",
        2836u16 => "image/vnd.mozilla.apng",
        2837u16 => "image/vnd.ms-modi",
        2838u16 => "image/vnd.net-fpx",
        2839u16 => "image/vnd.pco.b16",
        2840u16 => "image/vnd.radiance",
        2841u16 => "image/vnd.sealed.png",
        2842u16 => "image/vnd.sealedmedia.softseal.gif",
        2843u16 => "image/vnd.sealedmedia.softseal.jpg",
        2844u16 => "image/vnd.svf",
        2845u16 => "image/vnd.tencent.tap",
        2846u16 => "image/vnd.valve.source.texture",
        2847u16 => "image/vnd.wap.wbmp",
        2848u16 => "image/vnd.xiff",
        2849u16 => "image/vnd.zbrush.pcx",
        2850u16 => "image/webp",
        2851u16 => "image/wmf",
        2852u16 => "message/CPIM",
        2853u16 => "message/bhttp",
        2854u16 => "message/delivery-status",
        2855u16 => "message/disposition-notification",
        2856u16 => "message/example",
        2857u16 => "message/external-body",
        2858u16 => "message/feedback-report",
        2859u16 => "message/global",
        2860u16 => "message/global-delivery-status",
        2861u16 => "message/global-disposition-notification",
        2862u16 => "message/global-headers",
        2863u16 => "message/http",
        2864u16 => "message/imdn+xml",
        2865u16 => "message/mls",
        2866u16 => "message/news",
        2867u16 => "message/ohttp-req",
        2868u16 => "message/ohttp-res",
        2869u16 => "message/partial",
        2870u16 => "message/rfc822",
        2871u16 => "message/s-http",
        2872u16 => "message/sip",
        2873u16 => "message/sipfrag",
        2874u16 => "message/tracking-status",
        2875u16 => "message/vnd.si.simp",
        2876u16 => "message/vnd.wfa.wsc",
        2877u16 => "model/3mf",
        2878u16 => "model/JT",
        2879u16 => "model/e57",
        2880u16 => "model/example",
        2881u16 => "model/gltf+json",
        2882u16 => "model/gltf-binary",
        2883u16 => "model/iges",
        2884u16 => "model/mesh",
        2885u16 => "model/mtl",
        2886u16 => "model/obj",
        2887u16 => "model/prc",
        2888u16 => "model/step",
        2889u16 => "model/step+xml",
        2890u16 => "model/step+zip",
        2891u16 => "model/step-xml+zip",
        2892u16 => "model/stl",
        2893u16 => "model/u3d",
        2894u16 => "model/vnd.bary",
        2895u16 => "model/vnd.cld",
        2896u16 => "model/vnd.collada+xml",
        2897u16 => "model/vnd.dwf",
        2898u16 => "model/vnd.flatland.3dml",
        2899u16 => "model/vnd.gdl",
        2900u16 => "model/vnd.gs-gdl",
        2901u16 => "model/vnd.gtw",
        2902u16 => "model/vnd.moml+xml",
        2903u16 => "model/vnd.mts",
        2904u16 => "model/vnd.opengex",
        2905u16 => "model/vnd.parasolid.transmit.binary",
        2906u16 => "model/vnd.parasolid.transmit.text",
        2907u16 => "model/vnd.pytha.pyox",
        2908u16 => "model/vnd.rosette.annotated-data-model",
        2909u16 => "model/vnd.sap.vds",
        2910u16 => "model/vnd.usda",
        2911u16 => "model/vnd.usdz+zip",
        2912u16 => "model/vnd.valve.source.compiled-map",
        2913u16 => "model/vnd.vtu",
        2914u16 => "model/vrml",
        2915u16 => "model/x3d+fastinfoset",
        2916u16 => "model/x3d+xml",
        2917u16 => "model/x3d-vrml",
        2918u16 => "multipart/alternative",
        2919u16 => "multipart/appledouble",
        2920u16 => "multipart/byteranges",
        2921u16 => "multipart/digest",
        2922u16 => "multipart/encrypted",
        2923u16 => "multipart/example",
        2924u16 => "multipart/form-data",
        2925u16 => "multipart/header-set",
        2926u16 => "multipart/mixed",
        2927u16 => "multipart/multilingual",
        2928u16 => "multipart/parallel",
        2929u16 => "multipart/related",
        2930u16 => "multipart/report",
        2931u16 => "multipart/signed",
        2932u16 => "multipart/vnd.bint.med-plus",
        2933u16 => "multipart/voice-message",
        2934u16 => "multipart/x-mixed-replace",
        2935u16 => "text/1d-interleaved-parityfec",
        2936u16 => "text/RED",
        2937u16 => "text/SGML",
        2938u16 => "text/cache-manifest",
        2939u16 => "text/calendar",
        2940u16 => "text/cql",
        2941u16 => "text/cql-expression",
        2942u16 => "text/cql-identifier",
        2943u16 => "text/css",
        2944u16 => "text/csv",
        2945u16 => "text/csv-schema",
        2946u16 => "text/directory",
        2947u16 => "text/dns",
        2948u16 => "text/ecmascript",
        2949u16 => "text/encaprtp",
        2950u16 => "text/enriched",
        2951u16 => "text/example",
        2952u16 => "text/fhirpath",
        2953u16 => "text/flexfec",
        2954u16 => "text/fwdred",
        2955u16 => "text/gff3",
        2956u16 => "text/grammar-ref-list",
        2957u16 => "text/hl7v2",
        2958u16 => "text/html",
        2959u16 => "text/javascript",
        2960u16 => "text/jcr-cnd",
        2961u16 => "text/markdown",
        2962u16 => "text/mizar",
        2963u16 => "text/n3",
        2964u16 => "text/parameters",
        2965u16 => "text/parityfec",
        2966u16 => "text/plain",
        2967u16 => "text/provenance-notation",
        2968u16 => "text/prs.fallenstein.rst",
        2969u16 => "text/prs.lines.tag",
        2970u16 => "text/prs.prop.logic",
        2971u16 => "text/prs.texi",
        2972u16 => "text/raptorfec",
        2973u16 => "text/rfc822-headers",
        2974u16 => "text/richtext",
        2975u16 => "text/rtf",
        2976u16 => "text/rtp-enc-aescm128",
        2977u16 => "text/rtploopback",
        2978u16 => "text/rtx",
        2979u16 => "text/shaclc",
        2980u16 => "text/shex",
        2981u16 => "text/spdx",
        2982u16 => "text/strings",
        2983u16 => "text/t140",
        2984u16 => "text/tab-separated-values",
        2985u16 => "text/troff",
        2986u16 => "text/turtle",
        2987u16 => "text/ulpfec",
        2988u16 => "text/uri-list",
        2989u16 => "text/vcard",
        2990u16 => "text/vnd.DMClientScript",
        2991u16 => "text/vnd.IPTC.NITF",
        2992u16 => "text/vnd.IPTC.NewsML",
        2993u16 => "text/vnd.a",
        2994u16 => "text/vnd.abc",
        2995u16 => "text/vnd.ascii-art",
        2996u16 => "text/vnd.curl",
        2997u16 => "text/vnd.debian.copyright",
        2998u16 => "text/vnd.dvb.subtitle",
        2999u16 => "text/vnd.esmertec.theme-descriptor",
        3000u16 => "text/vnd.exchangeable",
        3001u16 => "text/vnd.familysearch.gedcom",
        3002u16 => "text/vnd.ficlab.flt",
        3003u16 => "text/vnd.fly",
        3004u16 => "text/vnd.fmi.flexstor",
        3005u16 => "text/vnd.gml",
        3006u16 => "text/vnd.graphviz",
        3007u16 => "text/vnd.hans",
        3008u16 => "text/vnd.hgl",
        3009u16 => "text/vnd.in3d.3dml",
        3010u16 => "text/vnd.in3d.spot",
        3011u16 => "text/vnd.latex-z",
        3012u16 => "text/vnd.motorola.reflex",
        3013u16 => "text/vnd.ms-mediapackage",
        3014u16 => "text/vnd.net2phone.commcenter.command",
        3015u16 => "text/vnd.radisys.msml-basic-layout",
        3016u16 => "text/vnd.senx.warpscript",
        3017u16 => "text/vnd.si.uricatalogue",
        3018u16 => "text/vnd.sosi",
        3019u16 => "text/vnd.sun.j2me.app-descriptor",
        3020u16 => "text/vnd.trolltech.linguist",
        3021u16 => "text/vnd.wap.si",
        3022u16 => "text/vnd.wap.sl",
        3023u16 => "text/vnd.wap.wml",
        3024u16 => "text/vnd.wap.wmlscript",
        3025u16 => "text/vtt",
        3026u16 => "text/wgsl",
        3027u16 => "text/xml",
        3028u16 => "text/xml-external-parsed-entity",
        3029u16 => "video/1d-interleaved-parityfec",
        3030u16 => "video/3gpp",
        3031u16 => "video/3gpp-tt",
        3032u16 => "video/3gpp2",
        3033u16 => "video/AV1",
        3034u16 => "video/BMPEG",
        3035u16 => "video/BT656",
        3036u16 => "video/CelB",
        3037u16 => "video/DV",
        3038u16 => "video/FFV1",
        3039u16 => "video/H261",
        3040u16 => "video/H263",
        3041u16 => "video/H263-1998",
        3042u16 => "video/H263-2000",
        3043u16 => "video/H264",
        3044u16 => "video/H264-RCDO",
        3045u16 => "video/H264-SVC",
        3046u16 => "video/H265",
        3047u16 => "video/H266",
        3048u16 => "video/JPEG",
        3049u16 => "video/MP1S",
        3050u16 => "video/MP2P",
        3051u16 => "video/MP2T",
        3052u16 => "video/MP4V-ES",
        3053u16 => "video/MPV",
        3054u16 => "video/SMPTE292M",
        3055u16 => "video/VP8",
        3056u16 => "video/VP9",
        3057u16 => "video/encaprtp",
        3058u16 => "video/evc",
        3059u16 => "video/example",
        3060u16 => "video/flexfec",
        3061u16 => "video/iso.segment",
        3062u16 => "video/jpeg2000",
        3063u16 => "video/jxsv",
        3064u16 => "video/matroska",
        3065u16 => "video/matroska-3d",
        3066u16 => "video/mj2",
        3067u16 => "video/mp4",
        3068u16 => "video/mpeg",
        3069u16 => "video/mpeg4-generic",
        3070u16 => "video/nv",
        3071u16 => "video/ogg",
        3072u16 => "video/parityfec",
        3073u16 => "video/pointer",
        3074u16 => "video/quicktime",
        3075u16 => "video/raptorfec",
        3076u16 => "video/raw",
        3077u16 => "video/rtp-enc-aescm128",
        3078u16 => "video/rtploopback",
        3079u16 => "video/rtx",
        3080u16 => "video/scip",
        3081u16 => "video/smpte291",
        3082u16 => "video/ulpfec",
        3083u16 => "video/vc1",
        3084u16 => "video/vc2",
        3085u16 => "video/vnd.CCTV",
        3086u16 => "video/vnd.dece.hd",
        3087u16 => "video/vnd.dece.mobile",
        3088u16 => "video/vnd.dece.mp4",
        3089u16 => "video/vnd.dece.pd",
        3090u16 => "video/vnd.dece.sd",
        3091u16 => "video/vnd.dece.video",
        3092u16 => "video/vnd.directv.mpeg",
        3093u16 => "video/vnd.directv.mpeg-tts",
        3094u16 => "video/vnd.dlna.mpeg-tts",
        3095u16 => "video/vnd.dvb.file",
        3096u16 => "video/vnd.fvt",
        3097u16 => "video/vnd.hns.video",
        3098u16 => "video/vnd.iptvforum.1dparityfec-1010",
        3099u16 => "video/vnd.iptvforum.1dparityfec-2005",
        3100u16 => "video/vnd.iptvforum.2dparityfec-1010",
        3101u16 => "video/vnd.iptvforum.2dparityfec-2005",
        3102u16 => "video/vnd.iptvforum.ttsavc",
        3103u16 => "video/vnd.iptvforum.ttsmpeg2",
        3104u16 => "video/vnd.motorola.video",
        3105u16 => "video/vnd.motorola.videop",
        3106u16 => "video/vnd.mpegurl",
        3107u16 => "video/vnd.ms-playready.media.pyv",
        3108u16 => "video/vnd.nokia.interleaved-multimedia",
        3109u16 => "video/vnd.nokia.mp4vr",
        3110u16 => "video/vnd.nokia.videovoip",
        3111u16 => "video/vnd.objectvideo",
        3112u16 => "video/vnd.radgamettools.bink",
        3113u16 => "video/vnd.radgamettools.smacker",
        3114u16 => "video/vnd.sealed.mpeg1",
        3115u16 => "video/vnd.sealed.mpeg4",
        3116u16 => "video/vnd.sealed.swf",
        3117u16 => "video/vnd.sealedmedia.softseal.mov",
        3118u16 => "video/vnd.uvvu.mp4",
        3119u16 => "video/vnd.vivo",
        3120u16 => "video/vnd.youtube.yt",
    };

    pub(super) const KNOWN_STRING: phf::OrderedMap<&'static str, EncodingPrefix> = phf_ordered_map! {
        "application/1d-interleaved-parityfec" => 1024u16,
        "application/3gpdash-qoe-report+xml" => 1025u16,
        "application/3gpp-ims+xml" => 1026u16,
        "application/3gppHal+json" => 1027u16,
        "application/3gppHalForms+json" => 1028u16,
        "application/A2L" => 1029u16,
        "application/AML" => 1030u16,
        "application/ATF" => 1031u16,
        "application/ATFX" => 1032u16,
        "application/ATXML" => 1033u16,
        "application/CALS-1840" => 1034u16,
        "application/CDFX+XML" => 1035u16,
        "application/CEA" => 1036u16,
        "application/CSTAdata+xml" => 1037u16,
        "application/DCD" => 1038u16,
        "application/DII" => 1039u16,
        "application/DIT" => 1040u16,
        "application/EDI-X12" => 1041u16,
        "application/EDI-consent" => 1042u16,
        "application/EDIFACT" => 1043u16,
        "application/EmergencyCallData.Comment+xml" => 1044u16,
        "application/EmergencyCallData.Control+xml" => 1045u16,
        "application/EmergencyCallData.DeviceInfo+xml" => 1046u16,
        "application/EmergencyCallData.LegacyESN+json" => 1047u16,
        "application/EmergencyCallData.ProviderInfo+xml" => 1048u16,
        "application/EmergencyCallData.ServiceInfo+xml" => 1049u16,
        "application/EmergencyCallData.SubscriberInfo+xml" => 1050u16,
        "application/EmergencyCallData.VEDS+xml" => 1051u16,
        "application/EmergencyCallData.cap+xml" => 1052u16,
        "application/EmergencyCallData.eCall.MSD" => 1053u16,
        "application/H224" => 1054u16,
        "application/IOTP" => 1055u16,
        "application/ISUP" => 1056u16,
        "application/LXF" => 1057u16,
        "application/MF4" => 1058u16,
        "application/ODA" => 1059u16,
        "application/ODX" => 1060u16,
        "application/PDX" => 1061u16,
        "application/QSIG" => 1062u16,
        "application/SGML" => 1063u16,
        "application/TETRA_ISI" => 1064u16,
        "application/ace+cbor" => 1065u16,
        "application/ace+json" => 1066u16,
        "application/activemessage" => 1067u16,
        "application/activity+json" => 1068u16,
        "application/aif+cbor" => 1069u16,
        "application/aif+json" => 1070u16,
        "application/alto-cdni+json" => 1071u16,
        "application/alto-cdnifilter+json" => 1072u16,
        "application/alto-costmap+json" => 1073u16,
        "application/alto-costmapfilter+json" => 1074u16,
        "application/alto-directory+json" => 1075u16,
        "application/alto-endpointcost+json" => 1076u16,
        "application/alto-endpointcostparams+json" => 1077u16,
        "application/alto-endpointprop+json" => 1078u16,
        "application/alto-endpointpropparams+json" => 1079u16,
        "application/alto-error+json" => 1080u16,
        "application/alto-networkmap+json" => 1081u16,
        "application/alto-networkmapfilter+json" => 1082u16,
        "application/alto-propmap+json" => 1083u16,
        "application/alto-propmapparams+json" => 1084u16,
        "application/alto-tips+json" => 1085u16,
        "application/alto-tipsparams+json" => 1086u16,
        "application/alto-updatestreamcontrol+json" => 1087u16,
        "application/alto-updatestreamparams+json" => 1088u16,
        "application/andrew-inset" => 1089u16,
        "application/applefile" => 1090u16,
        "application/at+jwt" => 1091u16,
        "application/atom+xml" => 1092u16,
        "application/atomcat+xml" => 1093u16,
        "application/atomdeleted+xml" => 1094u16,
        "application/atomicmail" => 1095u16,
        "application/atomsvc+xml" => 1096u16,
        "application/atsc-dwd+xml" => 1097u16,
        "application/atsc-dynamic-event-message" => 1098u16,
        "application/atsc-held+xml" => 1099u16,
        "application/atsc-rdt+json" => 1100u16,
        "application/atsc-rsat+xml" => 1101u16,
        "application/auth-policy+xml" => 1102u16,
        "application/automationml-aml+xml" => 1103u16,
        "application/automationml-amlx+zip" => 1104u16,
        "application/bacnet-xdd+zip" => 1105u16,
        "application/batch-SMTP" => 1106u16,
        "application/beep+xml" => 1107u16,
        "application/c2pa" => 1108u16,
        "application/calendar+json" => 1109u16,
        "application/calendar+xml" => 1110u16,
        "application/call-completion" => 1111u16,
        "application/captive+json" => 1112u16,
        "application/cbor" => 1113u16,
        "application/cbor-seq" => 1114u16,
        "application/cccex" => 1115u16,
        "application/ccmp+xml" => 1116u16,
        "application/ccxml+xml" => 1117u16,
        "application/cda+xml" => 1118u16,
        "application/cdmi-capability" => 1119u16,
        "application/cdmi-container" => 1120u16,
        "application/cdmi-domain" => 1121u16,
        "application/cdmi-object" => 1122u16,
        "application/cdmi-queue" => 1123u16,
        "application/cdni" => 1124u16,
        "application/cea-2018+xml" => 1125u16,
        "application/cellml+xml" => 1126u16,
        "application/cfw" => 1127u16,
        "application/cid-edhoc+cbor-seq" => 1128u16,
        "application/city+json" => 1129u16,
        "application/clr" => 1130u16,
        "application/clue+xml" => 1131u16,
        "application/clue_info+xml" => 1132u16,
        "application/cms" => 1133u16,
        "application/cnrp+xml" => 1134u16,
        "application/coap-group+json" => 1135u16,
        "application/coap-payload" => 1136u16,
        "application/commonground" => 1137u16,
        "application/concise-problem-details+cbor" => 1138u16,
        "application/conference-info+xml" => 1139u16,
        "application/cose" => 1140u16,
        "application/cose-key" => 1141u16,
        "application/cose-key-set" => 1142u16,
        "application/cose-x509" => 1143u16,
        "application/cpl+xml" => 1144u16,
        "application/csrattrs" => 1145u16,
        "application/csta+xml" => 1146u16,
        "application/csvm+json" => 1147u16,
        "application/cwl" => 1148u16,
        "application/cwl+json" => 1149u16,
        "application/cwt" => 1150u16,
        "application/cybercash" => 1151u16,
        "application/dash+xml" => 1152u16,
        "application/dash-patch+xml" => 1153u16,
        "application/dashdelta" => 1154u16,
        "application/davmount+xml" => 1155u16,
        "application/dca-rft" => 1156u16,
        "application/dec-dx" => 1157u16,
        "application/dialog-info+xml" => 1158u16,
        "application/dicom" => 1159u16,
        "application/dicom+json" => 1160u16,
        "application/dicom+xml" => 1161u16,
        "application/dns" => 1162u16,
        "application/dns+json" => 1163u16,
        "application/dns-message" => 1164u16,
        "application/dots+cbor" => 1165u16,
        "application/dpop+jwt" => 1166u16,
        "application/dskpp+xml" => 1167u16,
        "application/dssc+der" => 1168u16,
        "application/dssc+xml" => 1169u16,
        "application/dvcs" => 1170u16,
        "application/ecmascript" => 1171u16,
        "application/edhoc+cbor-seq" => 1172u16,
        "application/efi" => 1173u16,
        "application/elm+json" => 1174u16,
        "application/elm+xml" => 1175u16,
        "application/emma+xml" => 1176u16,
        "application/emotionml+xml" => 1177u16,
        "application/encaprtp" => 1178u16,
        "application/epp+xml" => 1179u16,
        "application/epub+zip" => 1180u16,
        "application/eshop" => 1181u16,
        "application/example" => 1182u16,
        "application/exi" => 1183u16,
        "application/expect-ct-report+json" => 1184u16,
        "application/express" => 1185u16,
        "application/fastinfoset" => 1186u16,
        "application/fastsoap" => 1187u16,
        "application/fdf" => 1188u16,
        "application/fdt+xml" => 1189u16,
        "application/fhir+json" => 1190u16,
        "application/fhir+xml" => 1191u16,
        "application/fits" => 1192u16,
        "application/flexfec" => 1193u16,
        "application/font-sfnt" => 1194u16,
        "application/font-tdpfr" => 1195u16,
        "application/font-woff" => 1196u16,
        "application/framework-attributes+xml" => 1197u16,
        "application/geo+json" => 1198u16,
        "application/geo+json-seq" => 1199u16,
        "application/geopackage+sqlite3" => 1200u16,
        "application/geoxacml+json" => 1201u16,
        "application/geoxacml+xml" => 1202u16,
        "application/gltf-buffer" => 1203u16,
        "application/gml+xml" => 1204u16,
        "application/gzip" => 1205u16,
        "application/held+xml" => 1206u16,
        "application/hl7v2+xml" => 1207u16,
        "application/http" => 1208u16,
        "application/hyperstudio" => 1209u16,
        "application/ibe-key-request+xml" => 1210u16,
        "application/ibe-pkg-reply+xml" => 1211u16,
        "application/ibe-pp-data" => 1212u16,
        "application/iges" => 1213u16,
        "application/im-iscomposing+xml" => 1214u16,
        "application/index" => 1215u16,
        "application/index.cmd" => 1216u16,
        "application/index.obj" => 1217u16,
        "application/index.response" => 1218u16,
        "application/index.vnd" => 1219u16,
        "application/inkml+xml" => 1220u16,
        "application/ipfix" => 1221u16,
        "application/ipp" => 1222u16,
        "application/its+xml" => 1223u16,
        "application/java-archive" => 1224u16,
        "application/javascript" => 1225u16,
        "application/jf2feed+json" => 1226u16,
        "application/jose" => 1227u16,
        "application/jose+json" => 1228u16,
        "application/jrd+json" => 1229u16,
        "application/jscalendar+json" => 1230u16,
        "application/jscontact+json" => 1231u16,
        "application/json" => 1232u16,
        "application/json-patch+json" => 1233u16,
        "application/json-seq" => 1234u16,
        "application/jsonpath" => 1235u16,
        "application/jwk+json" => 1236u16,
        "application/jwk-set+json" => 1237u16,
        "application/jwt" => 1238u16,
        "application/kpml-request+xml" => 1239u16,
        "application/kpml-response+xml" => 1240u16,
        "application/ld+json" => 1241u16,
        "application/lgr+xml" => 1242u16,
        "application/link-format" => 1243u16,
        "application/linkset" => 1244u16,
        "application/linkset+json" => 1245u16,
        "application/load-control+xml" => 1246u16,
        "application/logout+jwt" => 1247u16,
        "application/lost+xml" => 1248u16,
        "application/lostsync+xml" => 1249u16,
        "application/lpf+zip" => 1250u16,
        "application/mac-binhex40" => 1251u16,
        "application/macwriteii" => 1252u16,
        "application/mads+xml" => 1253u16,
        "application/manifest+json" => 1254u16,
        "application/marc" => 1255u16,
        "application/marcxml+xml" => 1256u16,
        "application/mathematica" => 1257u16,
        "application/mathml+xml" => 1258u16,
        "application/mathml-content+xml" => 1259u16,
        "application/mathml-presentation+xml" => 1260u16,
        "application/mbms-associated-procedure-description+xml" => 1261u16,
        "application/mbms-deregister+xml" => 1262u16,
        "application/mbms-envelope+xml" => 1263u16,
        "application/mbms-msk+xml" => 1264u16,
        "application/mbms-msk-response+xml" => 1265u16,
        "application/mbms-protection-description+xml" => 1266u16,
        "application/mbms-reception-report+xml" => 1267u16,
        "application/mbms-register+xml" => 1268u16,
        "application/mbms-register-response+xml" => 1269u16,
        "application/mbms-schedule+xml" => 1270u16,
        "application/mbms-user-service-description+xml" => 1271u16,
        "application/mbox" => 1272u16,
        "application/media-policy-dataset+xml" => 1273u16,
        "application/media_control+xml" => 1274u16,
        "application/mediaservercontrol+xml" => 1275u16,
        "application/merge-patch+json" => 1276u16,
        "application/metalink4+xml" => 1277u16,
        "application/mets+xml" => 1278u16,
        "application/mikey" => 1279u16,
        "application/mipc" => 1280u16,
        "application/missing-blocks+cbor-seq" => 1281u16,
        "application/mmt-aei+xml" => 1282u16,
        "application/mmt-usd+xml" => 1283u16,
        "application/mods+xml" => 1284u16,
        "application/moss-keys" => 1285u16,
        "application/moss-signature" => 1286u16,
        "application/mosskey-data" => 1287u16,
        "application/mosskey-request" => 1288u16,
        "application/mp21" => 1289u16,
        "application/mp4" => 1290u16,
        "application/mpeg4-generic" => 1291u16,
        "application/mpeg4-iod" => 1292u16,
        "application/mpeg4-iod-xmt" => 1293u16,
        "application/mrb-consumer+xml" => 1294u16,
        "application/mrb-publish+xml" => 1295u16,
        "application/msc-ivr+xml" => 1296u16,
        "application/msc-mixer+xml" => 1297u16,
        "application/msword" => 1298u16,
        "application/mud+json" => 1299u16,
        "application/multipart-core" => 1300u16,
        "application/mxf" => 1301u16,
        "application/n-quads" => 1302u16,
        "application/n-triples" => 1303u16,
        "application/nasdata" => 1304u16,
        "application/news-checkgroups" => 1305u16,
        "application/news-groupinfo" => 1306u16,
        "application/news-transmission" => 1307u16,
        "application/nlsml+xml" => 1308u16,
        "application/node" => 1309u16,
        "application/nss" => 1310u16,
        "application/oauth-authz-req+jwt" => 1311u16,
        "application/oblivious-dns-message" => 1312u16,
        "application/ocsp-request" => 1313u16,
        "application/ocsp-response" => 1314u16,
        "application/octet-stream" => 1315u16,
        "application/odm+xml" => 1316u16,
        "application/oebps-package+xml" => 1317u16,
        "application/ogg" => 1318u16,
        "application/ohttp-keys" => 1319u16,
        "application/opc-nodeset+xml" => 1320u16,
        "application/oscore" => 1321u16,
        "application/oxps" => 1322u16,
        "application/p21" => 1323u16,
        "application/p21+zip" => 1324u16,
        "application/p2p-overlay+xml" => 1325u16,
        "application/parityfec" => 1326u16,
        "application/passport" => 1327u16,
        "application/patch-ops-error+xml" => 1328u16,
        "application/pdf" => 1329u16,
        "application/pem-certificate-chain" => 1330u16,
        "application/pgp-encrypted" => 1331u16,
        "application/pgp-keys" => 1332u16,
        "application/pgp-signature" => 1333u16,
        "application/pidf+xml" => 1334u16,
        "application/pidf-diff+xml" => 1335u16,
        "application/pkcs10" => 1336u16,
        "application/pkcs12" => 1337u16,
        "application/pkcs7-mime" => 1338u16,
        "application/pkcs7-signature" => 1339u16,
        "application/pkcs8" => 1340u16,
        "application/pkcs8-encrypted" => 1341u16,
        "application/pkix-attr-cert" => 1342u16,
        "application/pkix-cert" => 1343u16,
        "application/pkix-crl" => 1344u16,
        "application/pkix-pkipath" => 1345u16,
        "application/pkixcmp" => 1346u16,
        "application/pls+xml" => 1347u16,
        "application/poc-settings+xml" => 1348u16,
        "application/postscript" => 1349u16,
        "application/ppsp-tracker+json" => 1350u16,
        "application/private-token-issuer-directory" => 1351u16,
        "application/private-token-request" => 1352u16,
        "application/private-token-response" => 1353u16,
        "application/problem+json" => 1354u16,
        "application/problem+xml" => 1355u16,
        "application/provenance+xml" => 1356u16,
        "application/prs.alvestrand.titrax-sheet" => 1357u16,
        "application/prs.cww" => 1358u16,
        "application/prs.cyn" => 1359u16,
        "application/prs.hpub+zip" => 1360u16,
        "application/prs.implied-document+xml" => 1361u16,
        "application/prs.implied-executable" => 1362u16,
        "application/prs.implied-object+json" => 1363u16,
        "application/prs.implied-object+json-seq" => 1364u16,
        "application/prs.implied-object+yaml" => 1365u16,
        "application/prs.implied-structure" => 1366u16,
        "application/prs.nprend" => 1367u16,
        "application/prs.plucker" => 1368u16,
        "application/prs.rdf-xml-crypt" => 1369u16,
        "application/prs.vcfbzip2" => 1370u16,
        "application/prs.xsf+xml" => 1371u16,
        "application/pskc+xml" => 1372u16,
        "application/pvd+json" => 1373u16,
        "application/raptorfec" => 1374u16,
        "application/rdap+json" => 1375u16,
        "application/rdf+xml" => 1376u16,
        "application/reginfo+xml" => 1377u16,
        "application/relax-ng-compact-syntax" => 1378u16,
        "application/remote-printing" => 1379u16,
        "application/reputon+json" => 1380u16,
        "application/resource-lists+xml" => 1381u16,
        "application/resource-lists-diff+xml" => 1382u16,
        "application/rfc+xml" => 1383u16,
        "application/riscos" => 1384u16,
        "application/rlmi+xml" => 1385u16,
        "application/rls-services+xml" => 1386u16,
        "application/route-apd+xml" => 1387u16,
        "application/route-s-tsid+xml" => 1388u16,
        "application/route-usd+xml" => 1389u16,
        "application/rpki-checklist" => 1390u16,
        "application/rpki-ghostbusters" => 1391u16,
        "application/rpki-manifest" => 1392u16,
        "application/rpki-publication" => 1393u16,
        "application/rpki-roa" => 1394u16,
        "application/rpki-updown" => 1395u16,
        "application/rtf" => 1396u16,
        "application/rtploopback" => 1397u16,
        "application/rtx" => 1398u16,
        "application/samlassertion+xml" => 1399u16,
        "application/samlmetadata+xml" => 1400u16,
        "application/sarif+json" => 1401u16,
        "application/sarif-external-properties+json" => 1402u16,
        "application/sbe" => 1403u16,
        "application/sbml+xml" => 1404u16,
        "application/scaip+xml" => 1405u16,
        "application/scim+json" => 1406u16,
        "application/scvp-cv-request" => 1407u16,
        "application/scvp-cv-response" => 1408u16,
        "application/scvp-vp-request" => 1409u16,
        "application/scvp-vp-response" => 1410u16,
        "application/sdp" => 1411u16,
        "application/secevent+jwt" => 1412u16,
        "application/senml+cbor" => 1413u16,
        "application/senml+json" => 1414u16,
        "application/senml+xml" => 1415u16,
        "application/senml-etch+cbor" => 1416u16,
        "application/senml-etch+json" => 1417u16,
        "application/senml-exi" => 1418u16,
        "application/sensml+cbor" => 1419u16,
        "application/sensml+json" => 1420u16,
        "application/sensml+xml" => 1421u16,
        "application/sensml-exi" => 1422u16,
        "application/sep+xml" => 1423u16,
        "application/sep-exi" => 1424u16,
        "application/session-info" => 1425u16,
        "application/set-payment" => 1426u16,
        "application/set-payment-initiation" => 1427u16,
        "application/set-registration" => 1428u16,
        "application/set-registration-initiation" => 1429u16,
        "application/sgml-open-catalog" => 1430u16,
        "application/shf+xml" => 1431u16,
        "application/sieve" => 1432u16,
        "application/simple-filter+xml" => 1433u16,
        "application/simple-message-summary" => 1434u16,
        "application/simpleSymbolContainer" => 1435u16,
        "application/sipc" => 1436u16,
        "application/slate" => 1437u16,
        "application/smil" => 1438u16,
        "application/smil+xml" => 1439u16,
        "application/smpte336m" => 1440u16,
        "application/soap+fastinfoset" => 1441u16,
        "application/soap+xml" => 1442u16,
        "application/sparql-query" => 1443u16,
        "application/sparql-results+xml" => 1444u16,
        "application/spdx+json" => 1445u16,
        "application/spirits-event+xml" => 1446u16,
        "application/sql" => 1447u16,
        "application/srgs" => 1448u16,
        "application/srgs+xml" => 1449u16,
        "application/sru+xml" => 1450u16,
        "application/ssml+xml" => 1451u16,
        "application/stix+json" => 1452u16,
        "application/swid+cbor" => 1453u16,
        "application/swid+xml" => 1454u16,
        "application/tamp-apex-update" => 1455u16,
        "application/tamp-apex-update-confirm" => 1456u16,
        "application/tamp-community-update" => 1457u16,
        "application/tamp-community-update-confirm" => 1458u16,
        "application/tamp-error" => 1459u16,
        "application/tamp-sequence-adjust" => 1460u16,
        "application/tamp-sequence-adjust-confirm" => 1461u16,
        "application/tamp-status-query" => 1462u16,
        "application/tamp-status-response" => 1463u16,
        "application/tamp-update" => 1464u16,
        "application/tamp-update-confirm" => 1465u16,
        "application/taxii+json" => 1466u16,
        "application/td+json" => 1467u16,
        "application/tei+xml" => 1468u16,
        "application/thraud+xml" => 1469u16,
        "application/timestamp-query" => 1470u16,
        "application/timestamp-reply" => 1471u16,
        "application/timestamped-data" => 1472u16,
        "application/tlsrpt+gzip" => 1473u16,
        "application/tlsrpt+json" => 1474u16,
        "application/tm+json" => 1475u16,
        "application/tnauthlist" => 1476u16,
        "application/token-introspection+jwt" => 1477u16,
        "application/trickle-ice-sdpfrag" => 1478u16,
        "application/trig" => 1479u16,
        "application/ttml+xml" => 1480u16,
        "application/tve-trigger" => 1481u16,
        "application/tzif" => 1482u16,
        "application/tzif-leap" => 1483u16,
        "application/ulpfec" => 1484u16,
        "application/urc-grpsheet+xml" => 1485u16,
        "application/urc-ressheet+xml" => 1486u16,
        "application/urc-targetdesc+xml" => 1487u16,
        "application/urc-uisocketdesc+xml" => 1488u16,
        "application/vcard+json" => 1489u16,
        "application/vcard+xml" => 1490u16,
        "application/vemmi" => 1491u16,
        "application/vnd.1000minds.decision-model+xml" => 1492u16,
        "application/vnd.1ob" => 1493u16,
        "application/vnd.3M.Post-it-Notes" => 1494u16,
        "application/vnd.3gpp-prose+xml" => 1495u16,
        "application/vnd.3gpp-prose-pc3a+xml" => 1496u16,
        "application/vnd.3gpp-prose-pc3ach+xml" => 1497u16,
        "application/vnd.3gpp-prose-pc3ch+xml" => 1498u16,
        "application/vnd.3gpp-prose-pc8+xml" => 1499u16,
        "application/vnd.3gpp-v2x-local-service-information" => 1500u16,
        "application/vnd.3gpp.5gnas" => 1501u16,
        "application/vnd.3gpp.GMOP+xml" => 1502u16,
        "application/vnd.3gpp.SRVCC-info+xml" => 1503u16,
        "application/vnd.3gpp.access-transfer-events+xml" => 1504u16,
        "application/vnd.3gpp.bsf+xml" => 1505u16,
        "application/vnd.3gpp.crs+xml" => 1506u16,
        "application/vnd.3gpp.current-location-discovery+xml" => 1507u16,
        "application/vnd.3gpp.gtpc" => 1508u16,
        "application/vnd.3gpp.interworking-data" => 1509u16,
        "application/vnd.3gpp.lpp" => 1510u16,
        "application/vnd.3gpp.mc-signalling-ear" => 1511u16,
        "application/vnd.3gpp.mcdata-affiliation-command+xml" => 1512u16,
        "application/vnd.3gpp.mcdata-info+xml" => 1513u16,
        "application/vnd.3gpp.mcdata-msgstore-ctrl-request+xml" => 1514u16,
        "application/vnd.3gpp.mcdata-payload" => 1515u16,
        "application/vnd.3gpp.mcdata-regroup+xml" => 1516u16,
        "application/vnd.3gpp.mcdata-service-config+xml" => 1517u16,
        "application/vnd.3gpp.mcdata-signalling" => 1518u16,
        "application/vnd.3gpp.mcdata-ue-config+xml" => 1519u16,
        "application/vnd.3gpp.mcdata-user-profile+xml" => 1520u16,
        "application/vnd.3gpp.mcptt-affiliation-command+xml" => 1521u16,
        "application/vnd.3gpp.mcptt-floor-request+xml" => 1522u16,
        "application/vnd.3gpp.mcptt-info+xml" => 1523u16,
        "application/vnd.3gpp.mcptt-location-info+xml" => 1524u16,
        "application/vnd.3gpp.mcptt-mbms-usage-info+xml" => 1525u16,
        "application/vnd.3gpp.mcptt-regroup+xml" => 1526u16,
        "application/vnd.3gpp.mcptt-service-config+xml" => 1527u16,
        "application/vnd.3gpp.mcptt-signed+xml" => 1528u16,
        "application/vnd.3gpp.mcptt-ue-config+xml" => 1529u16,
        "application/vnd.3gpp.mcptt-ue-init-config+xml" => 1530u16,
        "application/vnd.3gpp.mcptt-user-profile+xml" => 1531u16,
        "application/vnd.3gpp.mcvideo-affiliation-command+xml" => 1532u16,
        "application/vnd.3gpp.mcvideo-affiliation-info+xml" => 1533u16,
        "application/vnd.3gpp.mcvideo-info+xml" => 1534u16,
        "application/vnd.3gpp.mcvideo-location-info+xml" => 1535u16,
        "application/vnd.3gpp.mcvideo-mbms-usage-info+xml" => 1536u16,
        "application/vnd.3gpp.mcvideo-regroup+xml" => 1537u16,
        "application/vnd.3gpp.mcvideo-service-config+xml" => 1538u16,
        "application/vnd.3gpp.mcvideo-transmission-request+xml" => 1539u16,
        "application/vnd.3gpp.mcvideo-ue-config+xml" => 1540u16,
        "application/vnd.3gpp.mcvideo-user-profile+xml" => 1541u16,
        "application/vnd.3gpp.mid-call+xml" => 1542u16,
        "application/vnd.3gpp.ngap" => 1543u16,
        "application/vnd.3gpp.pfcp" => 1544u16,
        "application/vnd.3gpp.pic-bw-large" => 1545u16,
        "application/vnd.3gpp.pic-bw-small" => 1546u16,
        "application/vnd.3gpp.pic-bw-var" => 1547u16,
        "application/vnd.3gpp.s1ap" => 1548u16,
        "application/vnd.3gpp.seal-group-doc+xml" => 1549u16,
        "application/vnd.3gpp.seal-info+xml" => 1550u16,
        "application/vnd.3gpp.seal-location-info+xml" => 1551u16,
        "application/vnd.3gpp.seal-mbms-usage-info+xml" => 1552u16,
        "application/vnd.3gpp.seal-network-QoS-management-info+xml" => 1553u16,
        "application/vnd.3gpp.seal-ue-config-info+xml" => 1554u16,
        "application/vnd.3gpp.seal-unicast-info+xml" => 1555u16,
        "application/vnd.3gpp.seal-user-profile-info+xml" => 1556u16,
        "application/vnd.3gpp.sms" => 1557u16,
        "application/vnd.3gpp.sms+xml" => 1558u16,
        "application/vnd.3gpp.srvcc-ext+xml" => 1559u16,
        "application/vnd.3gpp.state-and-event-info+xml" => 1560u16,
        "application/vnd.3gpp.ussd+xml" => 1561u16,
        "application/vnd.3gpp.v2x" => 1562u16,
        "application/vnd.3gpp.vae-info+xml" => 1563u16,
        "application/vnd.3gpp2.bcmcsinfo+xml" => 1564u16,
        "application/vnd.3gpp2.sms" => 1565u16,
        "application/vnd.3gpp2.tcap" => 1566u16,
        "application/vnd.3lightssoftware.imagescal" => 1567u16,
        "application/vnd.FloGraphIt" => 1568u16,
        "application/vnd.HandHeld-Entertainment+xml" => 1569u16,
        "application/vnd.Kinar" => 1570u16,
        "application/vnd.MFER" => 1571u16,
        "application/vnd.Mobius.DAF" => 1572u16,
        "application/vnd.Mobius.DIS" => 1573u16,
        "application/vnd.Mobius.MBK" => 1574u16,
        "application/vnd.Mobius.MQY" => 1575u16,
        "application/vnd.Mobius.MSL" => 1576u16,
        "application/vnd.Mobius.PLC" => 1577u16,
        "application/vnd.Mobius.TXF" => 1578u16,
        "application/vnd.Quark.QuarkXPress" => 1579u16,
        "application/vnd.RenLearn.rlprint" => 1580u16,
        "application/vnd.SimTech-MindMapper" => 1581u16,
        "application/vnd.accpac.simply.aso" => 1582u16,
        "application/vnd.accpac.simply.imp" => 1583u16,
        "application/vnd.acm.addressxfer+json" => 1584u16,
        "application/vnd.acm.chatbot+json" => 1585u16,
        "application/vnd.acucobol" => 1586u16,
        "application/vnd.acucorp" => 1587u16,
        "application/vnd.adobe.flash.movie" => 1588u16,
        "application/vnd.adobe.formscentral.fcdt" => 1589u16,
        "application/vnd.adobe.fxp" => 1590u16,
        "application/vnd.adobe.partial-upload" => 1591u16,
        "application/vnd.adobe.xdp+xml" => 1592u16,
        "application/vnd.aether.imp" => 1593u16,
        "application/vnd.afpc.afplinedata" => 1594u16,
        "application/vnd.afpc.afplinedata-pagedef" => 1595u16,
        "application/vnd.afpc.cmoca-cmresource" => 1596u16,
        "application/vnd.afpc.foca-charset" => 1597u16,
        "application/vnd.afpc.foca-codedfont" => 1598u16,
        "application/vnd.afpc.foca-codepage" => 1599u16,
        "application/vnd.afpc.modca" => 1600u16,
        "application/vnd.afpc.modca-cmtable" => 1601u16,
        "application/vnd.afpc.modca-formdef" => 1602u16,
        "application/vnd.afpc.modca-mediummap" => 1603u16,
        "application/vnd.afpc.modca-objectcontainer" => 1604u16,
        "application/vnd.afpc.modca-overlay" => 1605u16,
        "application/vnd.afpc.modca-pagesegment" => 1606u16,
        "application/vnd.age" => 1607u16,
        "application/vnd.ah-barcode" => 1608u16,
        "application/vnd.ahead.space" => 1609u16,
        "application/vnd.airzip.filesecure.azf" => 1610u16,
        "application/vnd.airzip.filesecure.azs" => 1611u16,
        "application/vnd.amadeus+json" => 1612u16,
        "application/vnd.amazon.mobi8-ebook" => 1613u16,
        "application/vnd.americandynamics.acc" => 1614u16,
        "application/vnd.amiga.ami" => 1615u16,
        "application/vnd.amundsen.maze+xml" => 1616u16,
        "application/vnd.android.ota" => 1617u16,
        "application/vnd.anki" => 1618u16,
        "application/vnd.anser-web-certificate-issue-initiation" => 1619u16,
        "application/vnd.antix.game-component" => 1620u16,
        "application/vnd.apache.arrow.file" => 1621u16,
        "application/vnd.apache.arrow.stream" => 1622u16,
        "application/vnd.apache.parquet" => 1623u16,
        "application/vnd.apache.thrift.binary" => 1624u16,
        "application/vnd.apache.thrift.compact" => 1625u16,
        "application/vnd.apache.thrift.json" => 1626u16,
        "application/vnd.apexlang" => 1627u16,
        "application/vnd.api+json" => 1628u16,
        "application/vnd.aplextor.warrp+json" => 1629u16,
        "application/vnd.apothekende.reservation+json" => 1630u16,
        "application/vnd.apple.installer+xml" => 1631u16,
        "application/vnd.apple.keynote" => 1632u16,
        "application/vnd.apple.mpegurl" => 1633u16,
        "application/vnd.apple.numbers" => 1634u16,
        "application/vnd.apple.pages" => 1635u16,
        "application/vnd.arastra.swi" => 1636u16,
        "application/vnd.aristanetworks.swi" => 1637u16,
        "application/vnd.artisan+json" => 1638u16,
        "application/vnd.artsquare" => 1639u16,
        "application/vnd.astraea-software.iota" => 1640u16,
        "application/vnd.audiograph" => 1641u16,
        "application/vnd.autopackage" => 1642u16,
        "application/vnd.avalon+json" => 1643u16,
        "application/vnd.avistar+xml" => 1644u16,
        "application/vnd.balsamiq.bmml+xml" => 1645u16,
        "application/vnd.balsamiq.bmpr" => 1646u16,
        "application/vnd.banana-accounting" => 1647u16,
        "application/vnd.bbf.usp.error" => 1648u16,
        "application/vnd.bbf.usp.msg" => 1649u16,
        "application/vnd.bbf.usp.msg+json" => 1650u16,
        "application/vnd.bekitzur-stech+json" => 1651u16,
        "application/vnd.belightsoft.lhzd+zip" => 1652u16,
        "application/vnd.belightsoft.lhzl+zip" => 1653u16,
        "application/vnd.bint.med-content" => 1654u16,
        "application/vnd.biopax.rdf+xml" => 1655u16,
        "application/vnd.blink-idb-value-wrapper" => 1656u16,
        "application/vnd.blueice.multipass" => 1657u16,
        "application/vnd.bluetooth.ep.oob" => 1658u16,
        "application/vnd.bluetooth.le.oob" => 1659u16,
        "application/vnd.bmi" => 1660u16,
        "application/vnd.bpf" => 1661u16,
        "application/vnd.bpf3" => 1662u16,
        "application/vnd.businessobjects" => 1663u16,
        "application/vnd.byu.uapi+json" => 1664u16,
        "application/vnd.bzip3" => 1665u16,
        "application/vnd.cab-jscript" => 1666u16,
        "application/vnd.canon-cpdl" => 1667u16,
        "application/vnd.canon-lips" => 1668u16,
        "application/vnd.capasystems-pg+json" => 1669u16,
        "application/vnd.cendio.thinlinc.clientconf" => 1670u16,
        "application/vnd.century-systems.tcp_stream" => 1671u16,
        "application/vnd.chemdraw+xml" => 1672u16,
        "application/vnd.chess-pgn" => 1673u16,
        "application/vnd.chipnuts.karaoke-mmd" => 1674u16,
        "application/vnd.ciedi" => 1675u16,
        "application/vnd.cinderella" => 1676u16,
        "application/vnd.cirpack.isdn-ext" => 1677u16,
        "application/vnd.citationstyles.style+xml" => 1678u16,
        "application/vnd.claymore" => 1679u16,
        "application/vnd.cloanto.rp9" => 1680u16,
        "application/vnd.clonk.c4group" => 1681u16,
        "application/vnd.cluetrust.cartomobile-config" => 1682u16,
        "application/vnd.cluetrust.cartomobile-config-pkg" => 1683u16,
        "application/vnd.cncf.helm.chart.content.v1.tar+gzip" => 1684u16,
        "application/vnd.cncf.helm.chart.provenance.v1.prov" => 1685u16,
        "application/vnd.cncf.helm.config.v1+json" => 1686u16,
        "application/vnd.coffeescript" => 1687u16,
        "application/vnd.collabio.xodocuments.document" => 1688u16,
        "application/vnd.collabio.xodocuments.document-template" => 1689u16,
        "application/vnd.collabio.xodocuments.presentation" => 1690u16,
        "application/vnd.collabio.xodocuments.presentation-template" => 1691u16,
        "application/vnd.collabio.xodocuments.spreadsheet" => 1692u16,
        "application/vnd.collabio.xodocuments.spreadsheet-template" => 1693u16,
        "application/vnd.collection+json" => 1694u16,
        "application/vnd.collection.doc+json" => 1695u16,
        "application/vnd.collection.next+json" => 1696u16,
        "application/vnd.comicbook+zip" => 1697u16,
        "application/vnd.comicbook-rar" => 1698u16,
        "application/vnd.commerce-battelle" => 1699u16,
        "application/vnd.commonspace" => 1700u16,
        "application/vnd.contact.cmsg" => 1701u16,
        "application/vnd.coreos.ignition+json" => 1702u16,
        "application/vnd.cosmocaller" => 1703u16,
        "application/vnd.crick.clicker" => 1704u16,
        "application/vnd.crick.clicker.keyboard" => 1705u16,
        "application/vnd.crick.clicker.palette" => 1706u16,
        "application/vnd.crick.clicker.template" => 1707u16,
        "application/vnd.crick.clicker.wordbank" => 1708u16,
        "application/vnd.criticaltools.wbs+xml" => 1709u16,
        "application/vnd.cryptii.pipe+json" => 1710u16,
        "application/vnd.crypto-shade-file" => 1711u16,
        "application/vnd.cryptomator.encrypted" => 1712u16,
        "application/vnd.cryptomator.vault" => 1713u16,
        "application/vnd.ctc-posml" => 1714u16,
        "application/vnd.ctct.ws+xml" => 1715u16,
        "application/vnd.cups-pdf" => 1716u16,
        "application/vnd.cups-postscript" => 1717u16,
        "application/vnd.cups-ppd" => 1718u16,
        "application/vnd.cups-raster" => 1719u16,
        "application/vnd.cups-raw" => 1720u16,
        "application/vnd.curl" => 1721u16,
        "application/vnd.cyan.dean.root+xml" => 1722u16,
        "application/vnd.cybank" => 1723u16,
        "application/vnd.cyclonedx+json" => 1724u16,
        "application/vnd.cyclonedx+xml" => 1725u16,
        "application/vnd.d2l.coursepackage1p0+zip" => 1726u16,
        "application/vnd.d3m-dataset" => 1727u16,
        "application/vnd.d3m-problem" => 1728u16,
        "application/vnd.dart" => 1729u16,
        "application/vnd.data-vision.rdz" => 1730u16,
        "application/vnd.datalog" => 1731u16,
        "application/vnd.datapackage+json" => 1732u16,
        "application/vnd.dataresource+json" => 1733u16,
        "application/vnd.dbf" => 1734u16,
        "application/vnd.debian.binary-package" => 1735u16,
        "application/vnd.dece.data" => 1736u16,
        "application/vnd.dece.ttml+xml" => 1737u16,
        "application/vnd.dece.unspecified" => 1738u16,
        "application/vnd.dece.zip" => 1739u16,
        "application/vnd.denovo.fcselayout-link" => 1740u16,
        "application/vnd.desmume.movie" => 1741u16,
        "application/vnd.dir-bi.plate-dl-nosuffix" => 1742u16,
        "application/vnd.dm.delegation+xml" => 1743u16,
        "application/vnd.dna" => 1744u16,
        "application/vnd.document+json" => 1745u16,
        "application/vnd.dolby.mobile.1" => 1746u16,
        "application/vnd.dolby.mobile.2" => 1747u16,
        "application/vnd.doremir.scorecloud-binary-document" => 1748u16,
        "application/vnd.dpgraph" => 1749u16,
        "application/vnd.dreamfactory" => 1750u16,
        "application/vnd.drive+json" => 1751u16,
        "application/vnd.dtg.local" => 1752u16,
        "application/vnd.dtg.local.flash" => 1753u16,
        "application/vnd.dtg.local.html" => 1754u16,
        "application/vnd.dvb.ait" => 1755u16,
        "application/vnd.dvb.dvbisl+xml" => 1756u16,
        "application/vnd.dvb.dvbj" => 1757u16,
        "application/vnd.dvb.esgcontainer" => 1758u16,
        "application/vnd.dvb.ipdcdftnotifaccess" => 1759u16,
        "application/vnd.dvb.ipdcesgaccess" => 1760u16,
        "application/vnd.dvb.ipdcesgaccess2" => 1761u16,
        "application/vnd.dvb.ipdcesgpdd" => 1762u16,
        "application/vnd.dvb.ipdcroaming" => 1763u16,
        "application/vnd.dvb.iptv.alfec-base" => 1764u16,
        "application/vnd.dvb.iptv.alfec-enhancement" => 1765u16,
        "application/vnd.dvb.notif-aggregate-root+xml" => 1766u16,
        "application/vnd.dvb.notif-container+xml" => 1767u16,
        "application/vnd.dvb.notif-generic+xml" => 1768u16,
        "application/vnd.dvb.notif-ia-msglist+xml" => 1769u16,
        "application/vnd.dvb.notif-ia-registration-request+xml" => 1770u16,
        "application/vnd.dvb.notif-ia-registration-response+xml" => 1771u16,
        "application/vnd.dvb.notif-init+xml" => 1772u16,
        "application/vnd.dvb.pfr" => 1773u16,
        "application/vnd.dvb.service" => 1774u16,
        "application/vnd.dxr" => 1775u16,
        "application/vnd.dynageo" => 1776u16,
        "application/vnd.dzr" => 1777u16,
        "application/vnd.easykaraoke.cdgdownload" => 1778u16,
        "application/vnd.ecdis-update" => 1779u16,
        "application/vnd.ecip.rlp" => 1780u16,
        "application/vnd.eclipse.ditto+json" => 1781u16,
        "application/vnd.ecowin.chart" => 1782u16,
        "application/vnd.ecowin.filerequest" => 1783u16,
        "application/vnd.ecowin.fileupdate" => 1784u16,
        "application/vnd.ecowin.series" => 1785u16,
        "application/vnd.ecowin.seriesrequest" => 1786u16,
        "application/vnd.ecowin.seriesupdate" => 1787u16,
        "application/vnd.efi.img" => 1788u16,
        "application/vnd.efi.iso" => 1789u16,
        "application/vnd.eln+zip" => 1790u16,
        "application/vnd.emclient.accessrequest+xml" => 1791u16,
        "application/vnd.enliven" => 1792u16,
        "application/vnd.enphase.envoy" => 1793u16,
        "application/vnd.eprints.data+xml" => 1794u16,
        "application/vnd.epson.esf" => 1795u16,
        "application/vnd.epson.msf" => 1796u16,
        "application/vnd.epson.quickanime" => 1797u16,
        "application/vnd.epson.salt" => 1798u16,
        "application/vnd.epson.ssf" => 1799u16,
        "application/vnd.ericsson.quickcall" => 1800u16,
        "application/vnd.erofs" => 1801u16,
        "application/vnd.espass-espass+zip" => 1802u16,
        "application/vnd.eszigno3+xml" => 1803u16,
        "application/vnd.etsi.aoc+xml" => 1804u16,
        "application/vnd.etsi.asic-e+zip" => 1805u16,
        "application/vnd.etsi.asic-s+zip" => 1806u16,
        "application/vnd.etsi.cug+xml" => 1807u16,
        "application/vnd.etsi.iptvcommand+xml" => 1808u16,
        "application/vnd.etsi.iptvdiscovery+xml" => 1809u16,
        "application/vnd.etsi.iptvprofile+xml" => 1810u16,
        "application/vnd.etsi.iptvsad-bc+xml" => 1811u16,
        "application/vnd.etsi.iptvsad-cod+xml" => 1812u16,
        "application/vnd.etsi.iptvsad-npvr+xml" => 1813u16,
        "application/vnd.etsi.iptvservice+xml" => 1814u16,
        "application/vnd.etsi.iptvsync+xml" => 1815u16,
        "application/vnd.etsi.iptvueprofile+xml" => 1816u16,
        "application/vnd.etsi.mcid+xml" => 1817u16,
        "application/vnd.etsi.mheg5" => 1818u16,
        "application/vnd.etsi.overload-control-policy-dataset+xml" => 1819u16,
        "application/vnd.etsi.pstn+xml" => 1820u16,
        "application/vnd.etsi.sci+xml" => 1821u16,
        "application/vnd.etsi.simservs+xml" => 1822u16,
        "application/vnd.etsi.timestamp-token" => 1823u16,
        "application/vnd.etsi.tsl+xml" => 1824u16,
        "application/vnd.etsi.tsl.der" => 1825u16,
        "application/vnd.eu.kasparian.car+json" => 1826u16,
        "application/vnd.eudora.data" => 1827u16,
        "application/vnd.evolv.ecig.profile" => 1828u16,
        "application/vnd.evolv.ecig.settings" => 1829u16,
        "application/vnd.evolv.ecig.theme" => 1830u16,
        "application/vnd.exstream-empower+zip" => 1831u16,
        "application/vnd.exstream-package" => 1832u16,
        "application/vnd.ezpix-album" => 1833u16,
        "application/vnd.ezpix-package" => 1834u16,
        "application/vnd.f-secure.mobile" => 1835u16,
        "application/vnd.familysearch.gedcom+zip" => 1836u16,
        "application/vnd.fastcopy-disk-image" => 1837u16,
        "application/vnd.fdsn.mseed" => 1838u16,
        "application/vnd.fdsn.seed" => 1839u16,
        "application/vnd.ffsns" => 1840u16,
        "application/vnd.ficlab.flb+zip" => 1841u16,
        "application/vnd.filmit.zfc" => 1842u16,
        "application/vnd.fints" => 1843u16,
        "application/vnd.firemonkeys.cloudcell" => 1844u16,
        "application/vnd.fluxtime.clip" => 1845u16,
        "application/vnd.font-fontforge-sfd" => 1846u16,
        "application/vnd.framemaker" => 1847u16,
        "application/vnd.freelog.comic" => 1848u16,
        "application/vnd.frogans.fnc" => 1849u16,
        "application/vnd.frogans.ltf" => 1850u16,
        "application/vnd.fsc.weblaunch" => 1851u16,
        "application/vnd.fujifilm.fb.docuworks" => 1852u16,
        "application/vnd.fujifilm.fb.docuworks.binder" => 1853u16,
        "application/vnd.fujifilm.fb.docuworks.container" => 1854u16,
        "application/vnd.fujifilm.fb.jfi+xml" => 1855u16,
        "application/vnd.fujitsu.oasys" => 1856u16,
        "application/vnd.fujitsu.oasys2" => 1857u16,
        "application/vnd.fujitsu.oasys3" => 1858u16,
        "application/vnd.fujitsu.oasysgp" => 1859u16,
        "application/vnd.fujitsu.oasysprs" => 1860u16,
        "application/vnd.fujixerox.ART-EX" => 1861u16,
        "application/vnd.fujixerox.ART4" => 1862u16,
        "application/vnd.fujixerox.HBPL" => 1863u16,
        "application/vnd.fujixerox.ddd" => 1864u16,
        "application/vnd.fujixerox.docuworks" => 1865u16,
        "application/vnd.fujixerox.docuworks.binder" => 1866u16,
        "application/vnd.fujixerox.docuworks.container" => 1867u16,
        "application/vnd.fut-misnet" => 1868u16,
        "application/vnd.futoin+cbor" => 1869u16,
        "application/vnd.futoin+json" => 1870u16,
        "application/vnd.fuzzysheet" => 1871u16,
        "application/vnd.genomatix.tuxedo" => 1872u16,
        "application/vnd.genozip" => 1873u16,
        "application/vnd.gentics.grd+json" => 1874u16,
        "application/vnd.gentoo.catmetadata+xml" => 1875u16,
        "application/vnd.gentoo.ebuild" => 1876u16,
        "application/vnd.gentoo.eclass" => 1877u16,
        "application/vnd.gentoo.gpkg" => 1878u16,
        "application/vnd.gentoo.manifest" => 1879u16,
        "application/vnd.gentoo.pkgmetadata+xml" => 1880u16,
        "application/vnd.gentoo.xpak" => 1881u16,
        "application/vnd.geo+json" => 1882u16,
        "application/vnd.geocube+xml" => 1883u16,
        "application/vnd.geogebra.file" => 1884u16,
        "application/vnd.geogebra.slides" => 1885u16,
        "application/vnd.geogebra.tool" => 1886u16,
        "application/vnd.geometry-explorer" => 1887u16,
        "application/vnd.geonext" => 1888u16,
        "application/vnd.geoplan" => 1889u16,
        "application/vnd.geospace" => 1890u16,
        "application/vnd.gerber" => 1891u16,
        "application/vnd.globalplatform.card-content-mgt" => 1892u16,
        "application/vnd.globalplatform.card-content-mgt-response" => 1893u16,
        "application/vnd.gmx" => 1894u16,
        "application/vnd.gnu.taler.exchange+json" => 1895u16,
        "application/vnd.gnu.taler.merchant+json" => 1896u16,
        "application/vnd.google-earth.kml+xml" => 1897u16,
        "application/vnd.google-earth.kmz" => 1898u16,
        "application/vnd.gov.sk.e-form+xml" => 1899u16,
        "application/vnd.gov.sk.e-form+zip" => 1900u16,
        "application/vnd.gov.sk.xmldatacontainer+xml" => 1901u16,
        "application/vnd.gpxsee.map+xml" => 1902u16,
        "application/vnd.grafeq" => 1903u16,
        "application/vnd.gridmp" => 1904u16,
        "application/vnd.groove-account" => 1905u16,
        "application/vnd.groove-help" => 1906u16,
        "application/vnd.groove-identity-message" => 1907u16,
        "application/vnd.groove-injector" => 1908u16,
        "application/vnd.groove-tool-message" => 1909u16,
        "application/vnd.groove-tool-template" => 1910u16,
        "application/vnd.groove-vcard" => 1911u16,
        "application/vnd.hal+json" => 1912u16,
        "application/vnd.hal+xml" => 1913u16,
        "application/vnd.hbci" => 1914u16,
        "application/vnd.hc+json" => 1915u16,
        "application/vnd.hcl-bireports" => 1916u16,
        "application/vnd.hdt" => 1917u16,
        "application/vnd.heroku+json" => 1918u16,
        "application/vnd.hhe.lesson-player" => 1919u16,
        "application/vnd.hp-HPGL" => 1920u16,
        "application/vnd.hp-PCL" => 1921u16,
        "application/vnd.hp-PCLXL" => 1922u16,
        "application/vnd.hp-hpid" => 1923u16,
        "application/vnd.hp-hps" => 1924u16,
        "application/vnd.hp-jlyt" => 1925u16,
        "application/vnd.hsl" => 1926u16,
        "application/vnd.httphone" => 1927u16,
        "application/vnd.hydrostatix.sof-data" => 1928u16,
        "application/vnd.hyper+json" => 1929u16,
        "application/vnd.hyper-item+json" => 1930u16,
        "application/vnd.hyperdrive+json" => 1931u16,
        "application/vnd.hzn-3d-crossword" => 1932u16,
        "application/vnd.ibm.MiniPay" => 1933u16,
        "application/vnd.ibm.afplinedata" => 1934u16,
        "application/vnd.ibm.electronic-media" => 1935u16,
        "application/vnd.ibm.modcap" => 1936u16,
        "application/vnd.ibm.rights-management" => 1937u16,
        "application/vnd.ibm.secure-container" => 1938u16,
        "application/vnd.iccprofile" => 1939u16,
        "application/vnd.ieee.1905" => 1940u16,
        "application/vnd.igloader" => 1941u16,
        "application/vnd.imagemeter.folder+zip" => 1942u16,
        "application/vnd.imagemeter.image+zip" => 1943u16,
        "application/vnd.immervision-ivp" => 1944u16,
        "application/vnd.immervision-ivu" => 1945u16,
        "application/vnd.ims.imsccv1p1" => 1946u16,
        "application/vnd.ims.imsccv1p2" => 1947u16,
        "application/vnd.ims.imsccv1p3" => 1948u16,
        "application/vnd.ims.lis.v2.result+json" => 1949u16,
        "application/vnd.ims.lti.v2.toolconsumerprofile+json" => 1950u16,
        "application/vnd.ims.lti.v2.toolproxy+json" => 1951u16,
        "application/vnd.ims.lti.v2.toolproxy.id+json" => 1952u16,
        "application/vnd.ims.lti.v2.toolsettings+json" => 1953u16,
        "application/vnd.ims.lti.v2.toolsettings.simple+json" => 1954u16,
        "application/vnd.informedcontrol.rms+xml" => 1955u16,
        "application/vnd.informix-visionary" => 1956u16,
        "application/vnd.infotech.project" => 1957u16,
        "application/vnd.infotech.project+xml" => 1958u16,
        "application/vnd.innopath.wamp.notification" => 1959u16,
        "application/vnd.insors.igm" => 1960u16,
        "application/vnd.intercon.formnet" => 1961u16,
        "application/vnd.intergeo" => 1962u16,
        "application/vnd.intertrust.digibox" => 1963u16,
        "application/vnd.intertrust.nncp" => 1964u16,
        "application/vnd.intu.qbo" => 1965u16,
        "application/vnd.intu.qfx" => 1966u16,
        "application/vnd.ipfs.ipns-record" => 1967u16,
        "application/vnd.ipld.car" => 1968u16,
        "application/vnd.ipld.dag-cbor" => 1969u16,
        "application/vnd.ipld.dag-json" => 1970u16,
        "application/vnd.ipld.raw" => 1971u16,
        "application/vnd.iptc.g2.catalogitem+xml" => 1972u16,
        "application/vnd.iptc.g2.conceptitem+xml" => 1973u16,
        "application/vnd.iptc.g2.knowledgeitem+xml" => 1974u16,
        "application/vnd.iptc.g2.newsitem+xml" => 1975u16,
        "application/vnd.iptc.g2.newsmessage+xml" => 1976u16,
        "application/vnd.iptc.g2.packageitem+xml" => 1977u16,
        "application/vnd.iptc.g2.planningitem+xml" => 1978u16,
        "application/vnd.ipunplugged.rcprofile" => 1979u16,
        "application/vnd.irepository.package+xml" => 1980u16,
        "application/vnd.is-xpr" => 1981u16,
        "application/vnd.isac.fcs" => 1982u16,
        "application/vnd.iso11783-10+zip" => 1983u16,
        "application/vnd.jam" => 1984u16,
        "application/vnd.japannet-directory-service" => 1985u16,
        "application/vnd.japannet-jpnstore-wakeup" => 1986u16,
        "application/vnd.japannet-payment-wakeup" => 1987u16,
        "application/vnd.japannet-registration" => 1988u16,
        "application/vnd.japannet-registration-wakeup" => 1989u16,
        "application/vnd.japannet-setstore-wakeup" => 1990u16,
        "application/vnd.japannet-verification" => 1991u16,
        "application/vnd.japannet-verification-wakeup" => 1992u16,
        "application/vnd.jcp.javame.midlet-rms" => 1993u16,
        "application/vnd.jisp" => 1994u16,
        "application/vnd.joost.joda-archive" => 1995u16,
        "application/vnd.jsk.isdn-ngn" => 1996u16,
        "application/vnd.kahootz" => 1997u16,
        "application/vnd.kde.karbon" => 1998u16,
        "application/vnd.kde.kchart" => 1999u16,
        "application/vnd.kde.kformula" => 2000u16,
        "application/vnd.kde.kivio" => 2001u16,
        "application/vnd.kde.kontour" => 2002u16,
        "application/vnd.kde.kpresenter" => 2003u16,
        "application/vnd.kde.kspread" => 2004u16,
        "application/vnd.kde.kword" => 2005u16,
        "application/vnd.kenameaapp" => 2006u16,
        "application/vnd.kidspiration" => 2007u16,
        "application/vnd.koan" => 2008u16,
        "application/vnd.kodak-descriptor" => 2009u16,
        "application/vnd.las" => 2010u16,
        "application/vnd.las.las+json" => 2011u16,
        "application/vnd.las.las+xml" => 2012u16,
        "application/vnd.laszip" => 2013u16,
        "application/vnd.ldev.productlicensing" => 2014u16,
        "application/vnd.leap+json" => 2015u16,
        "application/vnd.liberty-request+xml" => 2016u16,
        "application/vnd.llamagraphics.life-balance.desktop" => 2017u16,
        "application/vnd.llamagraphics.life-balance.exchange+xml" => 2018u16,
        "application/vnd.logipipe.circuit+zip" => 2019u16,
        "application/vnd.loom" => 2020u16,
        "application/vnd.lotus-1-2-3" => 2021u16,
        "application/vnd.lotus-approach" => 2022u16,
        "application/vnd.lotus-freelance" => 2023u16,
        "application/vnd.lotus-notes" => 2024u16,
        "application/vnd.lotus-organizer" => 2025u16,
        "application/vnd.lotus-screencam" => 2026u16,
        "application/vnd.lotus-wordpro" => 2027u16,
        "application/vnd.macports.portpkg" => 2028u16,
        "application/vnd.mapbox-vector-tile" => 2029u16,
        "application/vnd.marlin.drm.actiontoken+xml" => 2030u16,
        "application/vnd.marlin.drm.conftoken+xml" => 2031u16,
        "application/vnd.marlin.drm.license+xml" => 2032u16,
        "application/vnd.marlin.drm.mdcf" => 2033u16,
        "application/vnd.mason+json" => 2034u16,
        "application/vnd.maxar.archive.3tz+zip" => 2035u16,
        "application/vnd.maxmind.maxmind-db" => 2036u16,
        "application/vnd.mcd" => 2037u16,
        "application/vnd.mdl" => 2038u16,
        "application/vnd.mdl-mbsdf" => 2039u16,
        "application/vnd.medcalcdata" => 2040u16,
        "application/vnd.mediastation.cdkey" => 2041u16,
        "application/vnd.medicalholodeck.recordxr" => 2042u16,
        "application/vnd.meridian-slingshot" => 2043u16,
        "application/vnd.mermaid" => 2044u16,
        "application/vnd.mfmp" => 2045u16,
        "application/vnd.micro+json" => 2046u16,
        "application/vnd.micrografx.flo" => 2047u16,
        "application/vnd.micrografx.igx" => 2048u16,
        "application/vnd.microsoft.portable-executable" => 2049u16,
        "application/vnd.microsoft.windows.thumbnail-cache" => 2050u16,
        "application/vnd.miele+json" => 2051u16,
        "application/vnd.mif" => 2052u16,
        "application/vnd.minisoft-hp3000-save" => 2053u16,
        "application/vnd.mitsubishi.misty-guard.trustweb" => 2054u16,
        "application/vnd.modl" => 2055u16,
        "application/vnd.mophun.application" => 2056u16,
        "application/vnd.mophun.certificate" => 2057u16,
        "application/vnd.motorola.flexsuite" => 2058u16,
        "application/vnd.motorola.flexsuite.adsi" => 2059u16,
        "application/vnd.motorola.flexsuite.fis" => 2060u16,
        "application/vnd.motorola.flexsuite.gotap" => 2061u16,
        "application/vnd.motorola.flexsuite.kmr" => 2062u16,
        "application/vnd.motorola.flexsuite.ttc" => 2063u16,
        "application/vnd.motorola.flexsuite.wem" => 2064u16,
        "application/vnd.motorola.iprm" => 2065u16,
        "application/vnd.mozilla.xul+xml" => 2066u16,
        "application/vnd.ms-3mfdocument" => 2067u16,
        "application/vnd.ms-PrintDeviceCapabilities+xml" => 2068u16,
        "application/vnd.ms-PrintSchemaTicket+xml" => 2069u16,
        "application/vnd.ms-artgalry" => 2070u16,
        "application/vnd.ms-asf" => 2071u16,
        "application/vnd.ms-cab-compressed" => 2072u16,
        "application/vnd.ms-excel" => 2073u16,
        "application/vnd.ms-excel.addin.macroEnabled.12" => 2074u16,
        "application/vnd.ms-excel.sheet.binary.macroEnabled.12" => 2075u16,
        "application/vnd.ms-excel.sheet.macroEnabled.12" => 2076u16,
        "application/vnd.ms-excel.template.macroEnabled.12" => 2077u16,
        "application/vnd.ms-fontobject" => 2078u16,
        "application/vnd.ms-htmlhelp" => 2079u16,
        "application/vnd.ms-ims" => 2080u16,
        "application/vnd.ms-lrm" => 2081u16,
        "application/vnd.ms-office.activeX+xml" => 2082u16,
        "application/vnd.ms-officetheme" => 2083u16,
        "application/vnd.ms-playready.initiator+xml" => 2084u16,
        "application/vnd.ms-powerpoint" => 2085u16,
        "application/vnd.ms-powerpoint.addin.macroEnabled.12" => 2086u16,
        "application/vnd.ms-powerpoint.presentation.macroEnabled.12" => 2087u16,
        "application/vnd.ms-powerpoint.slide.macroEnabled.12" => 2088u16,
        "application/vnd.ms-powerpoint.slideshow.macroEnabled.12" => 2089u16,
        "application/vnd.ms-powerpoint.template.macroEnabled.12" => 2090u16,
        "application/vnd.ms-project" => 2091u16,
        "application/vnd.ms-tnef" => 2092u16,
        "application/vnd.ms-windows.devicepairing" => 2093u16,
        "application/vnd.ms-windows.nwprinting.oob" => 2094u16,
        "application/vnd.ms-windows.printerpairing" => 2095u16,
        "application/vnd.ms-windows.wsd.oob" => 2096u16,
        "application/vnd.ms-wmdrm.lic-chlg-req" => 2097u16,
        "application/vnd.ms-wmdrm.lic-resp" => 2098u16,
        "application/vnd.ms-wmdrm.meter-chlg-req" => 2099u16,
        "application/vnd.ms-wmdrm.meter-resp" => 2100u16,
        "application/vnd.ms-word.document.macroEnabled.12" => 2101u16,
        "application/vnd.ms-word.template.macroEnabled.12" => 2102u16,
        "application/vnd.ms-works" => 2103u16,
        "application/vnd.ms-wpl" => 2104u16,
        "application/vnd.ms-xpsdocument" => 2105u16,
        "application/vnd.msa-disk-image" => 2106u16,
        "application/vnd.mseq" => 2107u16,
        "application/vnd.msign" => 2108u16,
        "application/vnd.multiad.creator" => 2109u16,
        "application/vnd.multiad.creator.cif" => 2110u16,
        "application/vnd.music-niff" => 2111u16,
        "application/vnd.musician" => 2112u16,
        "application/vnd.muvee.style" => 2113u16,
        "application/vnd.mynfc" => 2114u16,
        "application/vnd.nacamar.ybrid+json" => 2115u16,
        "application/vnd.nato.bindingdataobject+cbor" => 2116u16,
        "application/vnd.nato.bindingdataobject+json" => 2117u16,
        "application/vnd.nato.bindingdataobject+xml" => 2118u16,
        "application/vnd.nato.openxmlformats-package.iepd+zip" => 2119u16,
        "application/vnd.ncd.control" => 2120u16,
        "application/vnd.ncd.reference" => 2121u16,
        "application/vnd.nearst.inv+json" => 2122u16,
        "application/vnd.nebumind.line" => 2123u16,
        "application/vnd.nervana" => 2124u16,
        "application/vnd.netfpx" => 2125u16,
        "application/vnd.neurolanguage.nlu" => 2126u16,
        "application/vnd.nimn" => 2127u16,
        "application/vnd.nintendo.nitro.rom" => 2128u16,
        "application/vnd.nintendo.snes.rom" => 2129u16,
        "application/vnd.nitf" => 2130u16,
        "application/vnd.noblenet-directory" => 2131u16,
        "application/vnd.noblenet-sealer" => 2132u16,
        "application/vnd.noblenet-web" => 2133u16,
        "application/vnd.nokia.catalogs" => 2134u16,
        "application/vnd.nokia.conml+wbxml" => 2135u16,
        "application/vnd.nokia.conml+xml" => 2136u16,
        "application/vnd.nokia.iSDS-radio-presets" => 2137u16,
        "application/vnd.nokia.iptv.config+xml" => 2138u16,
        "application/vnd.nokia.landmark+wbxml" => 2139u16,
        "application/vnd.nokia.landmark+xml" => 2140u16,
        "application/vnd.nokia.landmarkcollection+xml" => 2141u16,
        "application/vnd.nokia.n-gage.ac+xml" => 2142u16,
        "application/vnd.nokia.n-gage.data" => 2143u16,
        "application/vnd.nokia.n-gage.symbian.install" => 2144u16,
        "application/vnd.nokia.ncd" => 2145u16,
        "application/vnd.nokia.pcd+wbxml" => 2146u16,
        "application/vnd.nokia.pcd+xml" => 2147u16,
        "application/vnd.nokia.radio-preset" => 2148u16,
        "application/vnd.nokia.radio-presets" => 2149u16,
        "application/vnd.novadigm.EDM" => 2150u16,
        "application/vnd.novadigm.EDX" => 2151u16,
        "application/vnd.novadigm.EXT" => 2152u16,
        "application/vnd.ntt-local.content-share" => 2153u16,
        "application/vnd.ntt-local.file-transfer" => 2154u16,
        "application/vnd.ntt-local.ogw_remote-access" => 2155u16,
        "application/vnd.ntt-local.sip-ta_remote" => 2156u16,
        "application/vnd.ntt-local.sip-ta_tcp_stream" => 2157u16,
        "application/vnd.oai.workflows" => 2158u16,
        "application/vnd.oai.workflows+json" => 2159u16,
        "application/vnd.oai.workflows+yaml" => 2160u16,
        "application/vnd.oasis.opendocument.base" => 2161u16,
        "application/vnd.oasis.opendocument.chart" => 2162u16,
        "application/vnd.oasis.opendocument.chart-template" => 2163u16,
        "application/vnd.oasis.opendocument.database" => 2164u16,
        "application/vnd.oasis.opendocument.formula" => 2165u16,
        "application/vnd.oasis.opendocument.formula-template" => 2166u16,
        "application/vnd.oasis.opendocument.graphics" => 2167u16,
        "application/vnd.oasis.opendocument.graphics-template" => 2168u16,
        "application/vnd.oasis.opendocument.image" => 2169u16,
        "application/vnd.oasis.opendocument.image-template" => 2170u16,
        "application/vnd.oasis.opendocument.presentation" => 2171u16,
        "application/vnd.oasis.opendocument.presentation-template" => 2172u16,
        "application/vnd.oasis.opendocument.spreadsheet" => 2173u16,
        "application/vnd.oasis.opendocument.spreadsheet-template" => 2174u16,
        "application/vnd.oasis.opendocument.text" => 2175u16,
        "application/vnd.oasis.opendocument.text-master" => 2176u16,
        "application/vnd.oasis.opendocument.text-master-template" => 2177u16,
        "application/vnd.oasis.opendocument.text-template" => 2178u16,
        "application/vnd.oasis.opendocument.text-web" => 2179u16,
        "application/vnd.obn" => 2180u16,
        "application/vnd.ocf+cbor" => 2181u16,
        "application/vnd.oci.image.manifest.v1+json" => 2182u16,
        "application/vnd.oftn.l10n+json" => 2183u16,
        "application/vnd.oipf.contentaccessdownload+xml" => 2184u16,
        "application/vnd.oipf.contentaccessstreaming+xml" => 2185u16,
        "application/vnd.oipf.cspg-hexbinary" => 2186u16,
        "application/vnd.oipf.dae.svg+xml" => 2187u16,
        "application/vnd.oipf.dae.xhtml+xml" => 2188u16,
        "application/vnd.oipf.mippvcontrolmessage+xml" => 2189u16,
        "application/vnd.oipf.pae.gem" => 2190u16,
        "application/vnd.oipf.spdiscovery+xml" => 2191u16,
        "application/vnd.oipf.spdlist+xml" => 2192u16,
        "application/vnd.oipf.ueprofile+xml" => 2193u16,
        "application/vnd.oipf.userprofile+xml" => 2194u16,
        "application/vnd.olpc-sugar" => 2195u16,
        "application/vnd.oma-scws-config" => 2196u16,
        "application/vnd.oma-scws-http-request" => 2197u16,
        "application/vnd.oma-scws-http-response" => 2198u16,
        "application/vnd.oma.bcast.associated-procedure-parameter+xml" => 2199u16,
        "application/vnd.oma.bcast.drm-trigger+xml" => 2200u16,
        "application/vnd.oma.bcast.imd+xml" => 2201u16,
        "application/vnd.oma.bcast.ltkm" => 2202u16,
        "application/vnd.oma.bcast.notification+xml" => 2203u16,
        "application/vnd.oma.bcast.provisioningtrigger" => 2204u16,
        "application/vnd.oma.bcast.sgboot" => 2205u16,
        "application/vnd.oma.bcast.sgdd+xml" => 2206u16,
        "application/vnd.oma.bcast.sgdu" => 2207u16,
        "application/vnd.oma.bcast.simple-symbol-container" => 2208u16,
        "application/vnd.oma.bcast.smartcard-trigger+xml" => 2209u16,
        "application/vnd.oma.bcast.sprov+xml" => 2210u16,
        "application/vnd.oma.bcast.stkm" => 2211u16,
        "application/vnd.oma.cab-address-book+xml" => 2212u16,
        "application/vnd.oma.cab-feature-handler+xml" => 2213u16,
        "application/vnd.oma.cab-pcc+xml" => 2214u16,
        "application/vnd.oma.cab-subs-invite+xml" => 2215u16,
        "application/vnd.oma.cab-user-prefs+xml" => 2216u16,
        "application/vnd.oma.dcd" => 2217u16,
        "application/vnd.oma.dcdc" => 2218u16,
        "application/vnd.oma.dd2+xml" => 2219u16,
        "application/vnd.oma.drm.risd+xml" => 2220u16,
        "application/vnd.oma.group-usage-list+xml" => 2221u16,
        "application/vnd.oma.lwm2m+cbor" => 2222u16,
        "application/vnd.oma.lwm2m+json" => 2223u16,
        "application/vnd.oma.lwm2m+tlv" => 2224u16,
        "application/vnd.oma.pal+xml" => 2225u16,
        "application/vnd.oma.poc.detailed-progress-report+xml" => 2226u16,
        "application/vnd.oma.poc.final-report+xml" => 2227u16,
        "application/vnd.oma.poc.groups+xml" => 2228u16,
        "application/vnd.oma.poc.invocation-descriptor+xml" => 2229u16,
        "application/vnd.oma.poc.optimized-progress-report+xml" => 2230u16,
        "application/vnd.oma.push" => 2231u16,
        "application/vnd.oma.scidm.messages+xml" => 2232u16,
        "application/vnd.oma.xcap-directory+xml" => 2233u16,
        "application/vnd.omads-email+xml" => 2234u16,
        "application/vnd.omads-file+xml" => 2235u16,
        "application/vnd.omads-folder+xml" => 2236u16,
        "application/vnd.omaloc-supl-init" => 2237u16,
        "application/vnd.onepager" => 2238u16,
        "application/vnd.onepagertamp" => 2239u16,
        "application/vnd.onepagertamx" => 2240u16,
        "application/vnd.onepagertat" => 2241u16,
        "application/vnd.onepagertatp" => 2242u16,
        "application/vnd.onepagertatx" => 2243u16,
        "application/vnd.onvif.metadata" => 2244u16,
        "application/vnd.openblox.game+xml" => 2245u16,
        "application/vnd.openblox.game-binary" => 2246u16,
        "application/vnd.openeye.oeb" => 2247u16,
        "application/vnd.openstreetmap.data+xml" => 2248u16,
        "application/vnd.opentimestamps.ots" => 2249u16,
        "application/vnd.openxmlformats-officedocument.custom-properties+xml" => 2250u16,
        "application/vnd.openxmlformats-officedocument.customXmlProperties+xml" => 2251u16,
        "application/vnd.openxmlformats-officedocument.drawing+xml" => 2252u16,
        "application/vnd.openxmlformats-officedocument.drawingml.chart+xml" => 2253u16,
        "application/vnd.openxmlformats-officedocument.drawingml.chartshapes+xml" => 2254u16,
        "application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml" => 2255u16,
        "application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml" => 2256u16,
        "application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml" => 2257u16,
        "application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml" => 2258u16,
        "application/vnd.openxmlformats-officedocument.extended-properties+xml" => 2259u16,
        "application/vnd.openxmlformats-officedocument.presentationml.commentAuthors+xml" => 2260u16,
        "application/vnd.openxmlformats-officedocument.presentationml.comments+xml" => 2261u16,
        "application/vnd.openxmlformats-officedocument.presentationml.handoutMaster+xml" => 2262u16,
        "application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml" => 2263u16,
        "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml" => 2264u16,
        "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml" => 2265u16,
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => 2266u16,
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml" => 2267u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slide" => 2268u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slide+xml" => 2269u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml" => 2270u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml" => 2271u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slideUpdateInfo+xml" => 2272u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slideshow" => 2273u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml" => 2274u16,
        "application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml" => 2275u16,
        "application/vnd.openxmlformats-officedocument.presentationml.tags+xml" => 2276u16,
        "application/vnd.openxmlformats-officedocument.presentationml.template" => 2277u16,
        "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml" => 2278u16,
        "application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml" => 2279u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml" => 2280u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml" => 2281u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml" => 2282u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml" => 2283u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.dialogsheet+xml" => 2284u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml" => 2285u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml" => 2286u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml" => 2287u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml" => 2288u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.queryTable+xml" => 2289u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionHeaders+xml" => 2290u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionLog+xml" => 2291u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml" => 2292u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => 2293u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml" => 2294u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml" => 2295u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml" => 2296u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml" => 2297u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.tableSingleCells+xml" => 2298u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.template" => 2299u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml" => 2300u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.userNames+xml" => 2301u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.volatileDependencies+xml" => 2302u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml" => 2303u16,
        "application/vnd.openxmlformats-officedocument.theme+xml" => 2304u16,
        "application/vnd.openxmlformats-officedocument.themeOverride+xml" => 2305u16,
        "application/vnd.openxmlformats-officedocument.vmlDrawing" => 2306u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml" => 2307u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => 2308u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.glossary+xml" => 2309u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml" => 2310u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml" => 2311u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml" => 2312u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml" => 2313u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml" => 2314u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml" => 2315u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml" => 2316u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml" => 2317u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.template" => 2318u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml" => 2319u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.webSettings+xml" => 2320u16,
        "application/vnd.openxmlformats-package.core-properties+xml" => 2321u16,
        "application/vnd.openxmlformats-package.digital-signature-xmlsignature+xml" => 2322u16,
        "application/vnd.openxmlformats-package.relationships+xml" => 2323u16,
        "application/vnd.oracle.resource+json" => 2324u16,
        "application/vnd.orange.indata" => 2325u16,
        "application/vnd.osa.netdeploy" => 2326u16,
        "application/vnd.osgeo.mapguide.package" => 2327u16,
        "application/vnd.osgi.bundle" => 2328u16,
        "application/vnd.osgi.dp" => 2329u16,
        "application/vnd.osgi.subsystem" => 2330u16,
        "application/vnd.otps.ct-kip+xml" => 2331u16,
        "application/vnd.oxli.countgraph" => 2332u16,
        "application/vnd.pagerduty+json" => 2333u16,
        "application/vnd.palm" => 2334u16,
        "application/vnd.panoply" => 2335u16,
        "application/vnd.paos.xml" => 2336u16,
        "application/vnd.patentdive" => 2337u16,
        "application/vnd.patientecommsdoc" => 2338u16,
        "application/vnd.pawaafile" => 2339u16,
        "application/vnd.pcos" => 2340u16,
        "application/vnd.pg.format" => 2341u16,
        "application/vnd.pg.osasli" => 2342u16,
        "application/vnd.piaccess.application-licence" => 2343u16,
        "application/vnd.picsel" => 2344u16,
        "application/vnd.pmi.widget" => 2345u16,
        "application/vnd.poc.group-advertisement+xml" => 2346u16,
        "application/vnd.pocketlearn" => 2347u16,
        "application/vnd.powerbuilder6" => 2348u16,
        "application/vnd.powerbuilder6-s" => 2349u16,
        "application/vnd.powerbuilder7" => 2350u16,
        "application/vnd.powerbuilder7-s" => 2351u16,
        "application/vnd.powerbuilder75" => 2352u16,
        "application/vnd.powerbuilder75-s" => 2353u16,
        "application/vnd.preminet" => 2354u16,
        "application/vnd.previewsystems.box" => 2355u16,
        "application/vnd.proteus.magazine" => 2356u16,
        "application/vnd.psfs" => 2357u16,
        "application/vnd.pt.mundusmundi" => 2358u16,
        "application/vnd.publishare-delta-tree" => 2359u16,
        "application/vnd.pvi.ptid1" => 2360u16,
        "application/vnd.pwg-multiplexed" => 2361u16,
        "application/vnd.pwg-xhtml-print+xml" => 2362u16,
        "application/vnd.qualcomm.brew-app-res" => 2363u16,
        "application/vnd.quarantainenet" => 2364u16,
        "application/vnd.quobject-quoxdocument" => 2365u16,
        "application/vnd.radisys.moml+xml" => 2366u16,
        "application/vnd.radisys.msml+xml" => 2367u16,
        "application/vnd.radisys.msml-audit+xml" => 2368u16,
        "application/vnd.radisys.msml-audit-conf+xml" => 2369u16,
        "application/vnd.radisys.msml-audit-conn+xml" => 2370u16,
        "application/vnd.radisys.msml-audit-dialog+xml" => 2371u16,
        "application/vnd.radisys.msml-audit-stream+xml" => 2372u16,
        "application/vnd.radisys.msml-conf+xml" => 2373u16,
        "application/vnd.radisys.msml-dialog+xml" => 2374u16,
        "application/vnd.radisys.msml-dialog-base+xml" => 2375u16,
        "application/vnd.radisys.msml-dialog-fax-detect+xml" => 2376u16,
        "application/vnd.radisys.msml-dialog-fax-sendrecv+xml" => 2377u16,
        "application/vnd.radisys.msml-dialog-group+xml" => 2378u16,
        "application/vnd.radisys.msml-dialog-speech+xml" => 2379u16,
        "application/vnd.radisys.msml-dialog-transform+xml" => 2380u16,
        "application/vnd.rainstor.data" => 2381u16,
        "application/vnd.rapid" => 2382u16,
        "application/vnd.rar" => 2383u16,
        "application/vnd.realvnc.bed" => 2384u16,
        "application/vnd.recordare.musicxml" => 2385u16,
        "application/vnd.recordare.musicxml+xml" => 2386u16,
        "application/vnd.relpipe" => 2387u16,
        "application/vnd.resilient.logic" => 2388u16,
        "application/vnd.restful+json" => 2389u16,
        "application/vnd.rig.cryptonote" => 2390u16,
        "application/vnd.route66.link66+xml" => 2391u16,
        "application/vnd.rs-274x" => 2392u16,
        "application/vnd.ruckus.download" => 2393u16,
        "application/vnd.s3sms" => 2394u16,
        "application/vnd.sailingtracker.track" => 2395u16,
        "application/vnd.sar" => 2396u16,
        "application/vnd.sbm.cid" => 2397u16,
        "application/vnd.sbm.mid2" => 2398u16,
        "application/vnd.scribus" => 2399u16,
        "application/vnd.sealed.3df" => 2400u16,
        "application/vnd.sealed.csf" => 2401u16,
        "application/vnd.sealed.doc" => 2402u16,
        "application/vnd.sealed.eml" => 2403u16,
        "application/vnd.sealed.mht" => 2404u16,
        "application/vnd.sealed.net" => 2405u16,
        "application/vnd.sealed.ppt" => 2406u16,
        "application/vnd.sealed.tiff" => 2407u16,
        "application/vnd.sealed.xls" => 2408u16,
        "application/vnd.sealedmedia.softseal.html" => 2409u16,
        "application/vnd.sealedmedia.softseal.pdf" => 2410u16,
        "application/vnd.seemail" => 2411u16,
        "application/vnd.seis+json" => 2412u16,
        "application/vnd.sema" => 2413u16,
        "application/vnd.semd" => 2414u16,
        "application/vnd.semf" => 2415u16,
        "application/vnd.shade-save-file" => 2416u16,
        "application/vnd.shana.informed.formdata" => 2417u16,
        "application/vnd.shana.informed.formtemplate" => 2418u16,
        "application/vnd.shana.informed.interchange" => 2419u16,
        "application/vnd.shana.informed.package" => 2420u16,
        "application/vnd.shootproof+json" => 2421u16,
        "application/vnd.shopkick+json" => 2422u16,
        "application/vnd.shp" => 2423u16,
        "application/vnd.shx" => 2424u16,
        "application/vnd.sigrok.session" => 2425u16,
        "application/vnd.siren+json" => 2426u16,
        "application/vnd.smaf" => 2427u16,
        "application/vnd.smart.notebook" => 2428u16,
        "application/vnd.smart.teacher" => 2429u16,
        "application/vnd.smintio.portals.archive" => 2430u16,
        "application/vnd.snesdev-page-table" => 2431u16,
        "application/vnd.software602.filler.form+xml" => 2432u16,
        "application/vnd.software602.filler.form-xml-zip" => 2433u16,
        "application/vnd.solent.sdkm+xml" => 2434u16,
        "application/vnd.spotfire.dxp" => 2435u16,
        "application/vnd.spotfire.sfs" => 2436u16,
        "application/vnd.sqlite3" => 2437u16,
        "application/vnd.sss-cod" => 2438u16,
        "application/vnd.sss-dtf" => 2439u16,
        "application/vnd.sss-ntf" => 2440u16,
        "application/vnd.stepmania.package" => 2441u16,
        "application/vnd.stepmania.stepchart" => 2442u16,
        "application/vnd.street-stream" => 2443u16,
        "application/vnd.sun.wadl+xml" => 2444u16,
        "application/vnd.sus-calendar" => 2445u16,
        "application/vnd.svd" => 2446u16,
        "application/vnd.swiftview-ics" => 2447u16,
        "application/vnd.sybyl.mol2" => 2448u16,
        "application/vnd.sycle+xml" => 2449u16,
        "application/vnd.syft+json" => 2450u16,
        "application/vnd.syncml+xml" => 2451u16,
        "application/vnd.syncml.dm+wbxml" => 2452u16,
        "application/vnd.syncml.dm+xml" => 2453u16,
        "application/vnd.syncml.dm.notification" => 2454u16,
        "application/vnd.syncml.dmddf+wbxml" => 2455u16,
        "application/vnd.syncml.dmddf+xml" => 2456u16,
        "application/vnd.syncml.dmtnds+wbxml" => 2457u16,
        "application/vnd.syncml.dmtnds+xml" => 2458u16,
        "application/vnd.syncml.ds.notification" => 2459u16,
        "application/vnd.tableschema+json" => 2460u16,
        "application/vnd.tao.intent-module-archive" => 2461u16,
        "application/vnd.tcpdump.pcap" => 2462u16,
        "application/vnd.think-cell.ppttc+json" => 2463u16,
        "application/vnd.tmd.mediaflex.api+xml" => 2464u16,
        "application/vnd.tml" => 2465u16,
        "application/vnd.tmobile-livetv" => 2466u16,
        "application/vnd.tri.onesource" => 2467u16,
        "application/vnd.trid.tpt" => 2468u16,
        "application/vnd.triscape.mxs" => 2469u16,
        "application/vnd.trueapp" => 2470u16,
        "application/vnd.truedoc" => 2471u16,
        "application/vnd.ubisoft.webplayer" => 2472u16,
        "application/vnd.ufdl" => 2473u16,
        "application/vnd.uiq.theme" => 2474u16,
        "application/vnd.umajin" => 2475u16,
        "application/vnd.unity" => 2476u16,
        "application/vnd.uoml+xml" => 2477u16,
        "application/vnd.uplanet.alert" => 2478u16,
        "application/vnd.uplanet.alert-wbxml" => 2479u16,
        "application/vnd.uplanet.bearer-choice" => 2480u16,
        "application/vnd.uplanet.bearer-choice-wbxml" => 2481u16,
        "application/vnd.uplanet.cacheop" => 2482u16,
        "application/vnd.uplanet.cacheop-wbxml" => 2483u16,
        "application/vnd.uplanet.channel" => 2484u16,
        "application/vnd.uplanet.channel-wbxml" => 2485u16,
        "application/vnd.uplanet.list" => 2486u16,
        "application/vnd.uplanet.list-wbxml" => 2487u16,
        "application/vnd.uplanet.listcmd" => 2488u16,
        "application/vnd.uplanet.listcmd-wbxml" => 2489u16,
        "application/vnd.uplanet.signal" => 2490u16,
        "application/vnd.uri-map" => 2491u16,
        "application/vnd.valve.source.material" => 2492u16,
        "application/vnd.vcx" => 2493u16,
        "application/vnd.vd-study" => 2494u16,
        "application/vnd.vectorworks" => 2495u16,
        "application/vnd.vel+json" => 2496u16,
        "application/vnd.verimatrix.vcas" => 2497u16,
        "application/vnd.veritone.aion+json" => 2498u16,
        "application/vnd.veryant.thin" => 2499u16,
        "application/vnd.ves.encrypted" => 2500u16,
        "application/vnd.vidsoft.vidconference" => 2501u16,
        "application/vnd.visio" => 2502u16,
        "application/vnd.visionary" => 2503u16,
        "application/vnd.vividence.scriptfile" => 2504u16,
        "application/vnd.vsf" => 2505u16,
        "application/vnd.wap.sic" => 2506u16,
        "application/vnd.wap.slc" => 2507u16,
        "application/vnd.wap.wbxml" => 2508u16,
        "application/vnd.wap.wmlc" => 2509u16,
        "application/vnd.wap.wmlscriptc" => 2510u16,
        "application/vnd.wasmflow.wafl" => 2511u16,
        "application/vnd.webturbo" => 2512u16,
        "application/vnd.wfa.dpp" => 2513u16,
        "application/vnd.wfa.p2p" => 2514u16,
        "application/vnd.wfa.wsc" => 2515u16,
        "application/vnd.windows.devicepairing" => 2516u16,
        "application/vnd.wmc" => 2517u16,
        "application/vnd.wmf.bootstrap" => 2518u16,
        "application/vnd.wolfram.mathematica" => 2519u16,
        "application/vnd.wolfram.mathematica.package" => 2520u16,
        "application/vnd.wolfram.player" => 2521u16,
        "application/vnd.wordlift" => 2522u16,
        "application/vnd.wordperfect" => 2523u16,
        "application/vnd.wqd" => 2524u16,
        "application/vnd.wrq-hp3000-labelled" => 2525u16,
        "application/vnd.wt.stf" => 2526u16,
        "application/vnd.wv.csp+wbxml" => 2527u16,
        "application/vnd.wv.csp+xml" => 2528u16,
        "application/vnd.wv.ssp+xml" => 2529u16,
        "application/vnd.xacml+json" => 2530u16,
        "application/vnd.xara" => 2531u16,
        "application/vnd.xecrets-encrypted" => 2532u16,
        "application/vnd.xfdl" => 2533u16,
        "application/vnd.xfdl.webform" => 2534u16,
        "application/vnd.xmi+xml" => 2535u16,
        "application/vnd.xmpie.cpkg" => 2536u16,
        "application/vnd.xmpie.dpkg" => 2537u16,
        "application/vnd.xmpie.plan" => 2538u16,
        "application/vnd.xmpie.ppkg" => 2539u16,
        "application/vnd.xmpie.xlim" => 2540u16,
        "application/vnd.yamaha.hv-dic" => 2541u16,
        "application/vnd.yamaha.hv-script" => 2542u16,
        "application/vnd.yamaha.hv-voice" => 2543u16,
        "application/vnd.yamaha.openscoreformat" => 2544u16,
        "application/vnd.yamaha.openscoreformat.osfpvg+xml" => 2545u16,
        "application/vnd.yamaha.remote-setup" => 2546u16,
        "application/vnd.yamaha.smaf-audio" => 2547u16,
        "application/vnd.yamaha.smaf-phrase" => 2548u16,
        "application/vnd.yamaha.through-ngn" => 2549u16,
        "application/vnd.yamaha.tunnel-udpencap" => 2550u16,
        "application/vnd.yaoweme" => 2551u16,
        "application/vnd.yellowriver-custom-menu" => 2552u16,
        "application/vnd.youtube.yt" => 2553u16,
        "application/vnd.zul" => 2554u16,
        "application/vnd.zzazz.deck+xml" => 2555u16,
        "application/voicexml+xml" => 2556u16,
        "application/voucher-cms+json" => 2557u16,
        "application/vq-rtcpxr" => 2558u16,
        "application/wasm" => 2559u16,
        "application/watcherinfo+xml" => 2560u16,
        "application/webpush-options+json" => 2561u16,
        "application/whoispp-query" => 2562u16,
        "application/whoispp-response" => 2563u16,
        "application/widget" => 2564u16,
        "application/wita" => 2565u16,
        "application/wordperfect5.1" => 2566u16,
        "application/wsdl+xml" => 2567u16,
        "application/wspolicy+xml" => 2568u16,
        "application/x-pki-message" => 2569u16,
        "application/x-www-form-urlencoded" => 2570u16,
        "application/x-x509-ca-cert" => 2571u16,
        "application/x-x509-ca-ra-cert" => 2572u16,
        "application/x-x509-next-ca-cert" => 2573u16,
        "application/x400-bp" => 2574u16,
        "application/xacml+xml" => 2575u16,
        "application/xcap-att+xml" => 2576u16,
        "application/xcap-caps+xml" => 2577u16,
        "application/xcap-diff+xml" => 2578u16,
        "application/xcap-el+xml" => 2579u16,
        "application/xcap-error+xml" => 2580u16,
        "application/xcap-ns+xml" => 2581u16,
        "application/xcon-conference-info+xml" => 2582u16,
        "application/xcon-conference-info-diff+xml" => 2583u16,
        "application/xenc+xml" => 2584u16,
        "application/xfdf" => 2585u16,
        "application/xhtml+xml" => 2586u16,
        "application/xliff+xml" => 2587u16,
        "application/xml" => 2588u16,
        "application/xml-dtd" => 2589u16,
        "application/xml-external-parsed-entity" => 2590u16,
        "application/xml-patch+xml" => 2591u16,
        "application/xmpp+xml" => 2592u16,
        "application/xop+xml" => 2593u16,
        "application/xslt+xml" => 2594u16,
        "application/xv+xml" => 2595u16,
        "application/yaml" => 2596u16,
        "application/yang" => 2597u16,
        "application/yang-data+cbor" => 2598u16,
        "application/yang-data+json" => 2599u16,
        "application/yang-data+xml" => 2600u16,
        "application/yang-patch+json" => 2601u16,
        "application/yang-patch+xml" => 2602u16,
        "application/yang-sid+json" => 2603u16,
        "application/yin+xml" => 2604u16,
        "application/zip" => 2605u16,
        "application/zlib" => 2606u16,
        "application/zstd" => 2607u16,
        "audio/1d-interleaved-parityfec" => 2608u16,
        "audio/32kadpcm" => 2609u16,
        "audio/3gpp" => 2610u16,
        "audio/3gpp2" => 2611u16,
        "audio/AMR" => 2612u16,
        "audio/AMR-WB" => 2613u16,
        "audio/ATRAC-ADVANCED-LOSSLESS" => 2614u16,
        "audio/ATRAC-X" => 2615u16,
        "audio/ATRAC3" => 2616u16,
        "audio/BV16" => 2617u16,
        "audio/BV32" => 2618u16,
        "audio/CN" => 2619u16,
        "audio/DAT12" => 2620u16,
        "audio/DV" => 2621u16,
        "audio/DVI4" => 2622u16,
        "audio/EVRC" => 2623u16,
        "audio/EVRC-QCP" => 2624u16,
        "audio/EVRC0" => 2625u16,
        "audio/EVRC1" => 2626u16,
        "audio/EVRCB" => 2627u16,
        "audio/EVRCB0" => 2628u16,
        "audio/EVRCB1" => 2629u16,
        "audio/EVRCNW" => 2630u16,
        "audio/EVRCNW0" => 2631u16,
        "audio/EVRCNW1" => 2632u16,
        "audio/EVRCWB" => 2633u16,
        "audio/EVRCWB0" => 2634u16,
        "audio/EVRCWB1" => 2635u16,
        "audio/EVS" => 2636u16,
        "audio/G711-0" => 2637u16,
        "audio/G719" => 2638u16,
        "audio/G722" => 2639u16,
        "audio/G7221" => 2640u16,
        "audio/G723" => 2641u16,
        "audio/G726-16" => 2642u16,
        "audio/G726-24" => 2643u16,
        "audio/G726-32" => 2644u16,
        "audio/G726-40" => 2645u16,
        "audio/G728" => 2646u16,
        "audio/G729" => 2647u16,
        "audio/G7291" => 2648u16,
        "audio/G729D" => 2649u16,
        "audio/G729E" => 2650u16,
        "audio/GSM" => 2651u16,
        "audio/GSM-EFR" => 2652u16,
        "audio/GSM-HR-08" => 2653u16,
        "audio/L16" => 2654u16,
        "audio/L20" => 2655u16,
        "audio/L24" => 2656u16,
        "audio/L8" => 2657u16,
        "audio/LPC" => 2658u16,
        "audio/MELP" => 2659u16,
        "audio/MELP1200" => 2660u16,
        "audio/MELP2400" => 2661u16,
        "audio/MELP600" => 2662u16,
        "audio/MP4A-LATM" => 2663u16,
        "audio/MPA" => 2664u16,
        "audio/PCMA" => 2665u16,
        "audio/PCMA-WB" => 2666u16,
        "audio/PCMU" => 2667u16,
        "audio/PCMU-WB" => 2668u16,
        "audio/QCELP" => 2669u16,
        "audio/RED" => 2670u16,
        "audio/SMV" => 2671u16,
        "audio/SMV-QCP" => 2672u16,
        "audio/SMV0" => 2673u16,
        "audio/TETRA_ACELP" => 2674u16,
        "audio/TETRA_ACELP_BB" => 2675u16,
        "audio/TSVCIS" => 2676u16,
        "audio/UEMCLIP" => 2677u16,
        "audio/VDVI" => 2678u16,
        "audio/VMR-WB" => 2679u16,
        "audio/aac" => 2680u16,
        "audio/ac3" => 2681u16,
        "audio/amr-wb+" => 2682u16,
        "audio/aptx" => 2683u16,
        "audio/asc" => 2684u16,
        "audio/basic" => 2685u16,
        "audio/clearmode" => 2686u16,
        "audio/dls" => 2687u16,
        "audio/dsr-es201108" => 2688u16,
        "audio/dsr-es202050" => 2689u16,
        "audio/dsr-es202211" => 2690u16,
        "audio/dsr-es202212" => 2691u16,
        "audio/eac3" => 2692u16,
        "audio/encaprtp" => 2693u16,
        "audio/example" => 2694u16,
        "audio/flexfec" => 2695u16,
        "audio/fwdred" => 2696u16,
        "audio/iLBC" => 2697u16,
        "audio/ip-mr_v2.5" => 2698u16,
        "audio/matroska" => 2699u16,
        "audio/mhas" => 2700u16,
        "audio/mobile-xmf" => 2701u16,
        "audio/mp4" => 2702u16,
        "audio/mpa-robust" => 2703u16,
        "audio/mpeg" => 2704u16,
        "audio/mpeg4-generic" => 2705u16,
        "audio/ogg" => 2706u16,
        "audio/opus" => 2707u16,
        "audio/parityfec" => 2708u16,
        "audio/prs.sid" => 2709u16,
        "audio/raptorfec" => 2710u16,
        "audio/rtp-enc-aescm128" => 2711u16,
        "audio/rtp-midi" => 2712u16,
        "audio/rtploopback" => 2713u16,
        "audio/rtx" => 2714u16,
        "audio/scip" => 2715u16,
        "audio/sofa" => 2716u16,
        "audio/sp-midi" => 2717u16,
        "audio/speex" => 2718u16,
        "audio/t140c" => 2719u16,
        "audio/t38" => 2720u16,
        "audio/telephone-event" => 2721u16,
        "audio/tone" => 2722u16,
        "audio/ulpfec" => 2723u16,
        "audio/usac" => 2724u16,
        "audio/vnd.3gpp.iufp" => 2725u16,
        "audio/vnd.4SB" => 2726u16,
        "audio/vnd.CELP" => 2727u16,
        "audio/vnd.audiokoz" => 2728u16,
        "audio/vnd.cisco.nse" => 2729u16,
        "audio/vnd.cmles.radio-events" => 2730u16,
        "audio/vnd.cns.anp1" => 2731u16,
        "audio/vnd.cns.inf1" => 2732u16,
        "audio/vnd.dece.audio" => 2733u16,
        "audio/vnd.digital-winds" => 2734u16,
        "audio/vnd.dlna.adts" => 2735u16,
        "audio/vnd.dolby.heaac.1" => 2736u16,
        "audio/vnd.dolby.heaac.2" => 2737u16,
        "audio/vnd.dolby.mlp" => 2738u16,
        "audio/vnd.dolby.mps" => 2739u16,
        "audio/vnd.dolby.pl2" => 2740u16,
        "audio/vnd.dolby.pl2x" => 2741u16,
        "audio/vnd.dolby.pl2z" => 2742u16,
        "audio/vnd.dolby.pulse.1" => 2743u16,
        "audio/vnd.dra" => 2744u16,
        "audio/vnd.dts" => 2745u16,
        "audio/vnd.dts.hd" => 2746u16,
        "audio/vnd.dts.uhd" => 2747u16,
        "audio/vnd.dvb.file" => 2748u16,
        "audio/vnd.everad.plj" => 2749u16,
        "audio/vnd.hns.audio" => 2750u16,
        "audio/vnd.lucent.voice" => 2751u16,
        "audio/vnd.ms-playready.media.pya" => 2752u16,
        "audio/vnd.nokia.mobile-xmf" => 2753u16,
        "audio/vnd.nortel.vbk" => 2754u16,
        "audio/vnd.nuera.ecelp4800" => 2755u16,
        "audio/vnd.nuera.ecelp7470" => 2756u16,
        "audio/vnd.nuera.ecelp9600" => 2757u16,
        "audio/vnd.octel.sbc" => 2758u16,
        "audio/vnd.presonus.multitrack" => 2759u16,
        "audio/vnd.qcelp" => 2760u16,
        "audio/vnd.rhetorex.32kadpcm" => 2761u16,
        "audio/vnd.rip" => 2762u16,
        "audio/vnd.sealedmedia.softseal.mpeg" => 2763u16,
        "audio/vnd.vmx.cvsd" => 2764u16,
        "audio/vorbis" => 2765u16,
        "audio/vorbis-config" => 2766u16,
        "font/collection" => 2767u16,
        "font/otf" => 2768u16,
        "font/sfnt" => 2769u16,
        "font/ttf" => 2770u16,
        "font/woff" => 2771u16,
        "font/woff2" => 2772u16,
        "image/aces" => 2773u16,
        "image/apng" => 2774u16,
        "image/avci" => 2775u16,
        "image/avcs" => 2776u16,
        "image/avif" => 2777u16,
        "image/bmp" => 2778u16,
        "image/cgm" => 2779u16,
        "image/dicom-rle" => 2780u16,
        "image/dpx" => 2781u16,
        "image/emf" => 2782u16,
        "image/example" => 2783u16,
        "image/fits" => 2784u16,
        "image/g3fax" => 2785u16,
        "image/gif" => 2786u16,
        "image/heic" => 2787u16,
        "image/heic-sequence" => 2788u16,
        "image/heif" => 2789u16,
        "image/heif-sequence" => 2790u16,
        "image/hej2k" => 2791u16,
        "image/hsj2" => 2792u16,
        "image/ief" => 2793u16,
        "image/j2c" => 2794u16,
        "image/jls" => 2795u16,
        "image/jp2" => 2796u16,
        "image/jpeg" => 2797u16,
        "image/jph" => 2798u16,
        "image/jphc" => 2799u16,
        "image/jpm" => 2800u16,
        "image/jpx" => 2801u16,
        "image/jxr" => 2802u16,
        "image/jxrA" => 2803u16,
        "image/jxrS" => 2804u16,
        "image/jxs" => 2805u16,
        "image/jxsc" => 2806u16,
        "image/jxsi" => 2807u16,
        "image/jxss" => 2808u16,
        "image/ktx" => 2809u16,
        "image/ktx2" => 2810u16,
        "image/naplps" => 2811u16,
        "image/png" => 2812u16,
        "image/prs.btif" => 2813u16,
        "image/prs.pti" => 2814u16,
        "image/pwg-raster" => 2815u16,
        "image/svg+xml" => 2816u16,
        "image/t38" => 2817u16,
        "image/tiff" => 2818u16,
        "image/tiff-fx" => 2819u16,
        "image/vnd.adobe.photoshop" => 2820u16,
        "image/vnd.airzip.accelerator.azv" => 2821u16,
        "image/vnd.cns.inf2" => 2822u16,
        "image/vnd.dece.graphic" => 2823u16,
        "image/vnd.djvu" => 2824u16,
        "image/vnd.dvb.subtitle" => 2825u16,
        "image/vnd.dwg" => 2826u16,
        "image/vnd.dxf" => 2827u16,
        "image/vnd.fastbidsheet" => 2828u16,
        "image/vnd.fpx" => 2829u16,
        "image/vnd.fst" => 2830u16,
        "image/vnd.fujixerox.edmics-mmr" => 2831u16,
        "image/vnd.fujixerox.edmics-rlc" => 2832u16,
        "image/vnd.globalgraphics.pgb" => 2833u16,
        "image/vnd.microsoft.icon" => 2834u16,
        "image/vnd.mix" => 2835u16,
        "image/vnd.mozilla.apng" => 2836u16,
        "image/vnd.ms-modi" => 2837u16,
        "image/vnd.net-fpx" => 2838u16,
        "image/vnd.pco.b16" => 2839u16,
        "image/vnd.radiance" => 2840u16,
        "image/vnd.sealed.png" => 2841u16,
        "image/vnd.sealedmedia.softseal.gif" => 2842u16,
        "image/vnd.sealedmedia.softseal.jpg" => 2843u16,
        "image/vnd.svf" => 2844u16,
        "image/vnd.tencent.tap" => 2845u16,
        "image/vnd.valve.source.texture" => 2846u16,
        "image/vnd.wap.wbmp" => 2847u16,
        "image/vnd.xiff" => 2848u16,
        "image/vnd.zbrush.pcx" => 2849u16,
        "image/webp" => 2850u16,
        "image/wmf" => 2851u16,
        "message/CPIM" => 2852u16,
        "message/bhttp" => 2853u16,
        "message/delivery-status" => 2854u16,
        "message/disposition-notification" => 2855u16,
        "message/example" => 2856u16,
        "message/external-body" => 2857u16,
        "message/feedback-report" => 2858u16,
        "message/global" => 2859u16,
        "message/global-delivery-status" => 2860u16,
        "message/global-disposition-notification" => 2861u16,
        "message/global-headers" => 2862u16,
        "message/http" => 2863u16,
        "message/imdn+xml" => 2864u16,
        "message/mls" => 2865u16,
        "message/news" => 2866u16,
        "message/ohttp-req" => 2867u16,
        "message/ohttp-res" => 2868u16,
        "message/partial" => 2869u16,
        "message/rfc822" => 2870u16,
        "message/s-http" => 2871u16,
        "message/sip" => 2872u16,
        "message/sipfrag" => 2873u16,
        "message/tracking-status" => 2874u16,
        "message/vnd.si.simp" => 2875u16,
        "message/vnd.wfa.wsc" => 2876u16,
        "model/3mf" => 2877u16,
        "model/JT" => 2878u16,
        "model/e57" => 2879u16,
        "model/example" => 2880u16,
        "model/gltf+json" => 2881u16,
        "model/gltf-binary" => 2882u16,
        "model/iges" => 2883u16,
        "model/mesh" => 2884u16,
        "model/mtl" => 2885u16,
        "model/obj" => 2886u16,
        "model/prc" => 2887u16,
        "model/step" => 2888u16,
        "model/step+xml" => 2889u16,
        "model/step+zip" => 2890u16,
        "model/step-xml+zip" => 2891u16,
        "model/stl" => 2892u16,
        "model/u3d" => 2893u16,
        "model/vnd.bary" => 2894u16,
        "model/vnd.cld" => 2895u16,
        "model/vnd.collada+xml" => 2896u16,
        "model/vnd.dwf" => 2897u16,
        "model/vnd.flatland.3dml" => 2898u16,
        "model/vnd.gdl" => 2899u16,
        "model/vnd.gs-gdl" => 2900u16,
        "model/vnd.gtw" => 2901u16,
        "model/vnd.moml+xml" => 2902u16,
        "model/vnd.mts" => 2903u16,
        "model/vnd.opengex" => 2904u16,
        "model/vnd.parasolid.transmit.binary" => 2905u16,
        "model/vnd.parasolid.transmit.text" => 2906u16,
        "model/vnd.pytha.pyox" => 2907u16,
        "model/vnd.rosette.annotated-data-model" => 2908u16,
        "model/vnd.sap.vds" => 2909u16,
        "model/vnd.usda" => 2910u16,
        "model/vnd.usdz+zip" => 2911u16,
        "model/vnd.valve.source.compiled-map" => 2912u16,
        "model/vnd.vtu" => 2913u16,
        "model/vrml" => 2914u16,
        "model/x3d+fastinfoset" => 2915u16,
        "model/x3d+xml" => 2916u16,
        "model/x3d-vrml" => 2917u16,
        "multipart/alternative" => 2918u16,
        "multipart/appledouble" => 2919u16,
        "multipart/byteranges" => 2920u16,
        "multipart/digest" => 2921u16,
        "multipart/encrypted" => 2922u16,
        "multipart/example" => 2923u16,
        "multipart/form-data" => 2924u16,
        "multipart/header-set" => 2925u16,
        "multipart/mixed" => 2926u16,
        "multipart/multilingual" => 2927u16,
        "multipart/parallel" => 2928u16,
        "multipart/related" => 2929u16,
        "multipart/report" => 2930u16,
        "multipart/signed" => 2931u16,
        "multipart/vnd.bint.med-plus" => 2932u16,
        "multipart/voice-message" => 2933u16,
        "multipart/x-mixed-replace" => 2934u16,
        "text/1d-interleaved-parityfec" => 2935u16,
        "text/RED" => 2936u16,
        "text/SGML" => 2937u16,
        "text/cache-manifest" => 2938u16,
        "text/calendar" => 2939u16,
        "text/cql" => 2940u16,
        "text/cql-expression" => 2941u16,
        "text/cql-identifier" => 2942u16,
        "text/css" => 2943u16,
        "text/csv" => 2944u16,
        "text/csv-schema" => 2945u16,
        "text/directory" => 2946u16,
        "text/dns" => 2947u16,
        "text/ecmascript" => 2948u16,
        "text/encaprtp" => 2949u16,
        "text/enriched" => 2950u16,
        "text/example" => 2951u16,
        "text/fhirpath" => 2952u16,
        "text/flexfec" => 2953u16,
        "text/fwdred" => 2954u16,
        "text/gff3" => 2955u16,
        "text/grammar-ref-list" => 2956u16,
        "text/hl7v2" => 2957u16,
        "text/html" => 2958u16,
        "text/javascript" => 2959u16,
        "text/jcr-cnd" => 2960u16,
        "text/markdown" => 2961u16,
        "text/mizar" => 2962u16,
        "text/n3" => 2963u16,
        "text/parameters" => 2964u16,
        "text/parityfec" => 2965u16,
        "text/plain" => 2966u16,
        "text/provenance-notation" => 2967u16,
        "text/prs.fallenstein.rst" => 2968u16,
        "text/prs.lines.tag" => 2969u16,
        "text/prs.prop.logic" => 2970u16,
        "text/prs.texi" => 2971u16,
        "text/raptorfec" => 2972u16,
        "text/rfc822-headers" => 2973u16,
        "text/richtext" => 2974u16,
        "text/rtf" => 2975u16,
        "text/rtp-enc-aescm128" => 2976u16,
        "text/rtploopback" => 2977u16,
        "text/rtx" => 2978u16,
        "text/shaclc" => 2979u16,
        "text/shex" => 2980u16,
        "text/spdx" => 2981u16,
        "text/strings" => 2982u16,
        "text/t140" => 2983u16,
        "text/tab-separated-values" => 2984u16,
        "text/troff" => 2985u16,
        "text/turtle" => 2986u16,
        "text/ulpfec" => 2987u16,
        "text/uri-list" => 2988u16,
        "text/vcard" => 2989u16,
        "text/vnd.DMClientScript" => 2990u16,
        "text/vnd.IPTC.NITF" => 2991u16,
        "text/vnd.IPTC.NewsML" => 2992u16,
        "text/vnd.a" => 2993u16,
        "text/vnd.abc" => 2994u16,
        "text/vnd.ascii-art" => 2995u16,
        "text/vnd.curl" => 2996u16,
        "text/vnd.debian.copyright" => 2997u16,
        "text/vnd.dvb.subtitle" => 2998u16,
        "text/vnd.esmertec.theme-descriptor" => 2999u16,
        "text/vnd.exchangeable" => 3000u16,
        "text/vnd.familysearch.gedcom" => 3001u16,
        "text/vnd.ficlab.flt" => 3002u16,
        "text/vnd.fly" => 3003u16,
        "text/vnd.fmi.flexstor" => 3004u16,
        "text/vnd.gml" => 3005u16,
        "text/vnd.graphviz" => 3006u16,
        "text/vnd.hans" => 3007u16,
        "text/vnd.hgl" => 3008u16,
        "text/vnd.in3d.3dml" => 3009u16,
        "text/vnd.in3d.spot" => 3010u16,
        "text/vnd.latex-z" => 3011u16,
        "text/vnd.motorola.reflex" => 3012u16,
        "text/vnd.ms-mediapackage" => 3013u16,
        "text/vnd.net2phone.commcenter.command" => 3014u16,
        "text/vnd.radisys.msml-basic-layout" => 3015u16,
        "text/vnd.senx.warpscript" => 3016u16,
        "text/vnd.si.uricatalogue" => 3017u16,
        "text/vnd.sosi" => 3018u16,
        "text/vnd.sun.j2me.app-descriptor" => 3019u16,
        "text/vnd.trolltech.linguist" => 3020u16,
        "text/vnd.wap.si" => 3021u16,
        "text/vnd.wap.sl" => 3022u16,
        "text/vnd.wap.wml" => 3023u16,
        "text/vnd.wap.wmlscript" => 3024u16,
        "text/vtt" => 3025u16,
        "text/wgsl" => 3026u16,
        "text/xml" => 3027u16,
        "text/xml-external-parsed-entity" => 3028u16,
        "video/1d-interleaved-parityfec" => 3029u16,
        "video/3gpp" => 3030u16,
        "video/3gpp-tt" => 3031u16,
        "video/3gpp2" => 3032u16,
        "video/AV1" => 3033u16,
        "video/BMPEG" => 3034u16,
        "video/BT656" => 3035u16,
        "video/CelB" => 3036u16,
        "video/DV" => 3037u16,
        "video/FFV1" => 3038u16,
        "video/H261" => 3039u16,
        "video/H263" => 3040u16,
        "video/H263-1998" => 3041u16,
        "video/H263-2000" => 3042u16,
        "video/H264" => 3043u16,
        "video/H264-RCDO" => 3044u16,
        "video/H264-SVC" => 3045u16,
        "video/H265" => 3046u16,
        "video/H266" => 3047u16,
        "video/JPEG" => 3048u16,
        "video/MP1S" => 3049u16,
        "video/MP2P" => 3050u16,
        "video/MP2T" => 3051u16,
        "video/MP4V-ES" => 3052u16,
        "video/MPV" => 3053u16,
        "video/SMPTE292M" => 3054u16,
        "video/VP8" => 3055u16,
        "video/VP9" => 3056u16,
        "video/encaprtp" => 3057u16,
        "video/evc" => 3058u16,
        "video/example" => 3059u16,
        "video/flexfec" => 3060u16,
        "video/iso.segment" => 3061u16,
        "video/jpeg2000" => 3062u16,
        "video/jxsv" => 3063u16,
        "video/matroska" => 3064u16,
        "video/matroska-3d" => 3065u16,
        "video/mj2" => 3066u16,
        "video/mp4" => 3067u16,
        "video/mpeg" => 3068u16,
        "video/mpeg4-generic" => 3069u16,
        "video/nv" => 3070u16,
        "video/ogg" => 3071u16,
        "video/parityfec" => 3072u16,
        "video/pointer" => 3073u16,
        "video/quicktime" => 3074u16,
        "video/raptorfec" => 3075u16,
        "video/raw" => 3076u16,
        "video/rtp-enc-aescm128" => 3077u16,
        "video/rtploopback" => 3078u16,
        "video/rtx" => 3079u16,
        "video/scip" => 3080u16,
        "video/smpte291" => 3081u16,
        "video/ulpfec" => 3082u16,
        "video/vc1" => 3083u16,
        "video/vc2" => 3084u16,
        "video/vnd.CCTV" => 3085u16,
        "video/vnd.dece.hd" => 3086u16,
        "video/vnd.dece.mobile" => 3087u16,
        "video/vnd.dece.mp4" => 3088u16,
        "video/vnd.dece.pd" => 3089u16,
        "video/vnd.dece.sd" => 3090u16,
        "video/vnd.dece.video" => 3091u16,
        "video/vnd.directv.mpeg" => 3092u16,
        "video/vnd.directv.mpeg-tts" => 3093u16,
        "video/vnd.dlna.mpeg-tts" => 3094u16,
        "video/vnd.dvb.file" => 3095u16,
        "video/vnd.fvt" => 3096u16,
        "video/vnd.hns.video" => 3097u16,
        "video/vnd.iptvforum.1dparityfec-1010" => 3098u16,
        "video/vnd.iptvforum.1dparityfec-2005" => 3099u16,
        "video/vnd.iptvforum.2dparityfec-1010" => 3100u16,
        "video/vnd.iptvforum.2dparityfec-2005" => 3101u16,
        "video/vnd.iptvforum.ttsavc" => 3102u16,
        "video/vnd.iptvforum.ttsmpeg2" => 3103u16,
        "video/vnd.motorola.video" => 3104u16,
        "video/vnd.motorola.videop" => 3105u16,
        "video/vnd.mpegurl" => 3106u16,
        "video/vnd.ms-playready.media.pyv" => 3107u16,
        "video/vnd.nokia.interleaved-multimedia" => 3108u16,
        "video/vnd.nokia.mp4vr" => 3109u16,
        "video/vnd.nokia.videovoip" => 3110u16,
        "video/vnd.objectvideo" => 3111u16,
        "video/vnd.radgamettools.bink" => 3112u16,
        "video/vnd.radgamettools.smacker" => 3113u16,
        "video/vnd.sealed.mpeg1" => 3114u16,
        "video/vnd.sealed.mpeg4" => 3115u16,
        "video/vnd.sealed.swf" => 3116u16,
        "video/vnd.sealedmedia.softseal.mov" => 3117u16,
        "video/vnd.uvvu.mp4" => 3118u16,
        "video/vnd.vivo" => 3119u16,
        "video/vnd.youtube.yt" => 3120u16,
    };
}

#[derive(Clone, Copy, Debug)]
pub struct IanaEncoding;

impl IanaEncoding {
    pub const EMPTY: Encoding = Encoding::empty();
    pub const APPLICATION_1D_INTERLEAVED_PARITYFEC: Encoding =
        Encoding::new(prefix::APPLICATION_1D_INTERLEAVED_PARITYFEC);
    pub const APPLICATION_3GPDASH_QOE_REPORT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_3GPDASH_QOE_REPORT_XML);
    pub const APPLICATION_3GPP_IMS_XML: Encoding = Encoding::new(prefix::APPLICATION_3GPP_IMS_XML);
    pub const APPLICATION_3GPPHAL_JSON: Encoding = Encoding::new(prefix::APPLICATION_3GPPHAL_JSON);
    pub const APPLICATION_3GPPHALFORMS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_3GPPHALFORMS_JSON);
    pub const APPLICATION_A2L: Encoding = Encoding::new(prefix::APPLICATION_A2L);
    pub const APPLICATION_AML: Encoding = Encoding::new(prefix::APPLICATION_AML);
    pub const APPLICATION_ATF: Encoding = Encoding::new(prefix::APPLICATION_ATF);
    pub const APPLICATION_ATFX: Encoding = Encoding::new(prefix::APPLICATION_ATFX);
    pub const APPLICATION_ATXML: Encoding = Encoding::new(prefix::APPLICATION_ATXML);
    pub const APPLICATION_CALS_1840: Encoding = Encoding::new(prefix::APPLICATION_CALS_1840);
    pub const APPLICATION_CDFX_XML: Encoding = Encoding::new(prefix::APPLICATION_CDFX_XML);
    pub const APPLICATION_CEA: Encoding = Encoding::new(prefix::APPLICATION_CEA);
    pub const APPLICATION_CSTADATA_XML: Encoding = Encoding::new(prefix::APPLICATION_CSTADATA_XML);
    pub const APPLICATION_DCD: Encoding = Encoding::new(prefix::APPLICATION_DCD);
    pub const APPLICATION_DII: Encoding = Encoding::new(prefix::APPLICATION_DII);
    pub const APPLICATION_DIT: Encoding = Encoding::new(prefix::APPLICATION_DIT);
    pub const APPLICATION_EDI_X12: Encoding = Encoding::new(prefix::APPLICATION_EDI_X12);
    pub const APPLICATION_EDI_CONSENT: Encoding = Encoding::new(prefix::APPLICATION_EDI_CONSENT);
    pub const APPLICATION_EDIFACT: Encoding = Encoding::new(prefix::APPLICATION_EDIFACT);
    pub const APPLICATION_EMERGENCYCALLDATA_COMMENT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_EMERGENCYCALLDATA_COMMENT_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_CONTROL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_EMERGENCYCALLDATA_CONTROL_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_DEVICEINFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_EMERGENCYCALLDATA_DEVICEINFO_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_LEGACYESN_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_EMERGENCYCALLDATA_LEGACYESN_JSON);
    pub const APPLICATION_EMERGENCYCALLDATA_PROVIDERINFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_EMERGENCYCALLDATA_PROVIDERINFO_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_SERVICEINFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_EMERGENCYCALLDATA_SERVICEINFO_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_SUBSCRIBERINFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_EMERGENCYCALLDATA_SUBSCRIBERINFO_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_VEDS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_EMERGENCYCALLDATA_VEDS_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_CAP_XML: Encoding =
        Encoding::new(prefix::APPLICATION_EMERGENCYCALLDATA_CAP_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_ECALL_MSD: Encoding =
        Encoding::new(prefix::APPLICATION_EMERGENCYCALLDATA_ECALL_MSD);
    pub const APPLICATION_H224: Encoding = Encoding::new(prefix::APPLICATION_H224);
    pub const APPLICATION_IOTP: Encoding = Encoding::new(prefix::APPLICATION_IOTP);
    pub const APPLICATION_ISUP: Encoding = Encoding::new(prefix::APPLICATION_ISUP);
    pub const APPLICATION_LXF: Encoding = Encoding::new(prefix::APPLICATION_LXF);
    pub const APPLICATION_MF4: Encoding = Encoding::new(prefix::APPLICATION_MF4);
    pub const APPLICATION_ODA: Encoding = Encoding::new(prefix::APPLICATION_ODA);
    pub const APPLICATION_ODX: Encoding = Encoding::new(prefix::APPLICATION_ODX);
    pub const APPLICATION_PDX: Encoding = Encoding::new(prefix::APPLICATION_PDX);
    pub const APPLICATION_QSIG: Encoding = Encoding::new(prefix::APPLICATION_QSIG);
    pub const APPLICATION_SGML: Encoding = Encoding::new(prefix::APPLICATION_SGML);
    pub const APPLICATION_TETRA_ISI: Encoding = Encoding::new(prefix::APPLICATION_TETRA_ISI);
    pub const APPLICATION_ACE_CBOR: Encoding = Encoding::new(prefix::APPLICATION_ACE_CBOR);
    pub const APPLICATION_ACE_JSON: Encoding = Encoding::new(prefix::APPLICATION_ACE_JSON);
    pub const APPLICATION_ACTIVEMESSAGE: Encoding =
        Encoding::new(prefix::APPLICATION_ACTIVEMESSAGE);
    pub const APPLICATION_ACTIVITY_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ACTIVITY_JSON);
    pub const APPLICATION_AIF_CBOR: Encoding = Encoding::new(prefix::APPLICATION_AIF_CBOR);
    pub const APPLICATION_AIF_JSON: Encoding = Encoding::new(prefix::APPLICATION_AIF_JSON);
    pub const APPLICATION_ALTO_CDNI_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_CDNI_JSON);
    pub const APPLICATION_ALTO_CDNIFILTER_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_CDNIFILTER_JSON);
    pub const APPLICATION_ALTO_COSTMAP_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_COSTMAP_JSON);
    pub const APPLICATION_ALTO_COSTMAPFILTER_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_COSTMAPFILTER_JSON);
    pub const APPLICATION_ALTO_DIRECTORY_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_DIRECTORY_JSON);
    pub const APPLICATION_ALTO_ENDPOINTCOST_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_ENDPOINTCOST_JSON);
    pub const APPLICATION_ALTO_ENDPOINTCOSTPARAMS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_ENDPOINTCOSTPARAMS_JSON);
    pub const APPLICATION_ALTO_ENDPOINTPROP_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_ENDPOINTPROP_JSON);
    pub const APPLICATION_ALTO_ENDPOINTPROPPARAMS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_ENDPOINTPROPPARAMS_JSON);
    pub const APPLICATION_ALTO_ERROR_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_ERROR_JSON);
    pub const APPLICATION_ALTO_NETWORKMAP_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_NETWORKMAP_JSON);
    pub const APPLICATION_ALTO_NETWORKMAPFILTER_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_NETWORKMAPFILTER_JSON);
    pub const APPLICATION_ALTO_PROPMAP_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_PROPMAP_JSON);
    pub const APPLICATION_ALTO_PROPMAPPARAMS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_PROPMAPPARAMS_JSON);
    pub const APPLICATION_ALTO_TIPS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_TIPS_JSON);
    pub const APPLICATION_ALTO_TIPSPARAMS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_TIPSPARAMS_JSON);
    pub const APPLICATION_ALTO_UPDATESTREAMCONTROL_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_UPDATESTREAMCONTROL_JSON);
    pub const APPLICATION_ALTO_UPDATESTREAMPARAMS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ALTO_UPDATESTREAMPARAMS_JSON);
    pub const APPLICATION_ANDREW_INSET: Encoding = Encoding::new(prefix::APPLICATION_ANDREW_INSET);
    pub const APPLICATION_APPLEFILE: Encoding = Encoding::new(prefix::APPLICATION_APPLEFILE);
    pub const APPLICATION_AT_JWT: Encoding = Encoding::new(prefix::APPLICATION_AT_JWT);
    pub const APPLICATION_ATOM_XML: Encoding = Encoding::new(prefix::APPLICATION_ATOM_XML);
    pub const APPLICATION_ATOMCAT_XML: Encoding = Encoding::new(prefix::APPLICATION_ATOMCAT_XML);
    pub const APPLICATION_ATOMDELETED_XML: Encoding =
        Encoding::new(prefix::APPLICATION_ATOMDELETED_XML);
    pub const APPLICATION_ATOMICMAIL: Encoding = Encoding::new(prefix::APPLICATION_ATOMICMAIL);
    pub const APPLICATION_ATOMSVC_XML: Encoding = Encoding::new(prefix::APPLICATION_ATOMSVC_XML);
    pub const APPLICATION_ATSC_DWD_XML: Encoding = Encoding::new(prefix::APPLICATION_ATSC_DWD_XML);
    pub const APPLICATION_ATSC_DYNAMIC_EVENT_MESSAGE: Encoding =
        Encoding::new(prefix::APPLICATION_ATSC_DYNAMIC_EVENT_MESSAGE);
    pub const APPLICATION_ATSC_HELD_XML: Encoding =
        Encoding::new(prefix::APPLICATION_ATSC_HELD_XML);
    pub const APPLICATION_ATSC_RDT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_ATSC_RDT_JSON);
    pub const APPLICATION_ATSC_RSAT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_ATSC_RSAT_XML);
    pub const APPLICATION_AUTH_POLICY_XML: Encoding =
        Encoding::new(prefix::APPLICATION_AUTH_POLICY_XML);
    pub const APPLICATION_AUTOMATIONML_AML_XML: Encoding =
        Encoding::new(prefix::APPLICATION_AUTOMATIONML_AML_XML);
    pub const APPLICATION_AUTOMATIONML_AMLX_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_AUTOMATIONML_AMLX_ZIP);
    pub const APPLICATION_BACNET_XDD_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_BACNET_XDD_ZIP);
    pub const APPLICATION_BATCH_SMTP: Encoding = Encoding::new(prefix::APPLICATION_BATCH_SMTP);
    pub const APPLICATION_BEEP_XML: Encoding = Encoding::new(prefix::APPLICATION_BEEP_XML);
    pub const APPLICATION_C2PA: Encoding = Encoding::new(prefix::APPLICATION_C2PA);
    pub const APPLICATION_CALENDAR_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_CALENDAR_JSON);
    pub const APPLICATION_CALENDAR_XML: Encoding = Encoding::new(prefix::APPLICATION_CALENDAR_XML);
    pub const APPLICATION_CALL_COMPLETION: Encoding =
        Encoding::new(prefix::APPLICATION_CALL_COMPLETION);
    pub const APPLICATION_CAPTIVE_JSON: Encoding = Encoding::new(prefix::APPLICATION_CAPTIVE_JSON);
    pub const APPLICATION_CBOR: Encoding = Encoding::new(prefix::APPLICATION_CBOR);
    pub const APPLICATION_CBOR_SEQ: Encoding = Encoding::new(prefix::APPLICATION_CBOR_SEQ);
    pub const APPLICATION_CCCEX: Encoding = Encoding::new(prefix::APPLICATION_CCCEX);
    pub const APPLICATION_CCMP_XML: Encoding = Encoding::new(prefix::APPLICATION_CCMP_XML);
    pub const APPLICATION_CCXML_XML: Encoding = Encoding::new(prefix::APPLICATION_CCXML_XML);
    pub const APPLICATION_CDA_XML: Encoding = Encoding::new(prefix::APPLICATION_CDA_XML);
    pub const APPLICATION_CDMI_CAPABILITY: Encoding =
        Encoding::new(prefix::APPLICATION_CDMI_CAPABILITY);
    pub const APPLICATION_CDMI_CONTAINER: Encoding =
        Encoding::new(prefix::APPLICATION_CDMI_CONTAINER);
    pub const APPLICATION_CDMI_DOMAIN: Encoding = Encoding::new(prefix::APPLICATION_CDMI_DOMAIN);
    pub const APPLICATION_CDMI_OBJECT: Encoding = Encoding::new(prefix::APPLICATION_CDMI_OBJECT);
    pub const APPLICATION_CDMI_QUEUE: Encoding = Encoding::new(prefix::APPLICATION_CDMI_QUEUE);
    pub const APPLICATION_CDNI: Encoding = Encoding::new(prefix::APPLICATION_CDNI);
    pub const APPLICATION_CEA_2018_XML: Encoding = Encoding::new(prefix::APPLICATION_CEA_2018_XML);
    pub const APPLICATION_CELLML_XML: Encoding = Encoding::new(prefix::APPLICATION_CELLML_XML);
    pub const APPLICATION_CFW: Encoding = Encoding::new(prefix::APPLICATION_CFW);
    pub const APPLICATION_CID_EDHOC_CBOR_SEQ: Encoding =
        Encoding::new(prefix::APPLICATION_CID_EDHOC_CBOR_SEQ);
    pub const APPLICATION_CITY_JSON: Encoding = Encoding::new(prefix::APPLICATION_CITY_JSON);
    pub const APPLICATION_CLR: Encoding = Encoding::new(prefix::APPLICATION_CLR);
    pub const APPLICATION_CLUE_XML: Encoding = Encoding::new(prefix::APPLICATION_CLUE_XML);
    pub const APPLICATION_CLUE_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_CLUE_INFO_XML);
    pub const APPLICATION_CMS: Encoding = Encoding::new(prefix::APPLICATION_CMS);
    pub const APPLICATION_CNRP_XML: Encoding = Encoding::new(prefix::APPLICATION_CNRP_XML);
    pub const APPLICATION_COAP_GROUP_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_COAP_GROUP_JSON);
    pub const APPLICATION_COAP_PAYLOAD: Encoding = Encoding::new(prefix::APPLICATION_COAP_PAYLOAD);
    pub const APPLICATION_COMMONGROUND: Encoding = Encoding::new(prefix::APPLICATION_COMMONGROUND);
    pub const APPLICATION_CONCISE_PROBLEM_DETAILS_CBOR: Encoding =
        Encoding::new(prefix::APPLICATION_CONCISE_PROBLEM_DETAILS_CBOR);
    pub const APPLICATION_CONFERENCE_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_CONFERENCE_INFO_XML);
    pub const APPLICATION_COSE: Encoding = Encoding::new(prefix::APPLICATION_COSE);
    pub const APPLICATION_COSE_KEY: Encoding = Encoding::new(prefix::APPLICATION_COSE_KEY);
    pub const APPLICATION_COSE_KEY_SET: Encoding = Encoding::new(prefix::APPLICATION_COSE_KEY_SET);
    pub const APPLICATION_COSE_X509: Encoding = Encoding::new(prefix::APPLICATION_COSE_X509);
    pub const APPLICATION_CPL_XML: Encoding = Encoding::new(prefix::APPLICATION_CPL_XML);
    pub const APPLICATION_CSRATTRS: Encoding = Encoding::new(prefix::APPLICATION_CSRATTRS);
    pub const APPLICATION_CSTA_XML: Encoding = Encoding::new(prefix::APPLICATION_CSTA_XML);
    pub const APPLICATION_CSVM_JSON: Encoding = Encoding::new(prefix::APPLICATION_CSVM_JSON);
    pub const APPLICATION_CWL: Encoding = Encoding::new(prefix::APPLICATION_CWL);
    pub const APPLICATION_CWL_JSON: Encoding = Encoding::new(prefix::APPLICATION_CWL_JSON);
    pub const APPLICATION_CWT: Encoding = Encoding::new(prefix::APPLICATION_CWT);
    pub const APPLICATION_CYBERCASH: Encoding = Encoding::new(prefix::APPLICATION_CYBERCASH);
    pub const APPLICATION_DASH_XML: Encoding = Encoding::new(prefix::APPLICATION_DASH_XML);
    pub const APPLICATION_DASH_PATCH_XML: Encoding =
        Encoding::new(prefix::APPLICATION_DASH_PATCH_XML);
    pub const APPLICATION_DASHDELTA: Encoding = Encoding::new(prefix::APPLICATION_DASHDELTA);
    pub const APPLICATION_DAVMOUNT_XML: Encoding = Encoding::new(prefix::APPLICATION_DAVMOUNT_XML);
    pub const APPLICATION_DCA_RFT: Encoding = Encoding::new(prefix::APPLICATION_DCA_RFT);
    pub const APPLICATION_DEC_DX: Encoding = Encoding::new(prefix::APPLICATION_DEC_DX);
    pub const APPLICATION_DIALOG_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_DIALOG_INFO_XML);
    pub const APPLICATION_DICOM: Encoding = Encoding::new(prefix::APPLICATION_DICOM);
    pub const APPLICATION_DICOM_JSON: Encoding = Encoding::new(prefix::APPLICATION_DICOM_JSON);
    pub const APPLICATION_DICOM_XML: Encoding = Encoding::new(prefix::APPLICATION_DICOM_XML);
    pub const APPLICATION_DNS: Encoding = Encoding::new(prefix::APPLICATION_DNS);
    pub const APPLICATION_DNS_JSON: Encoding = Encoding::new(prefix::APPLICATION_DNS_JSON);
    pub const APPLICATION_DNS_MESSAGE: Encoding = Encoding::new(prefix::APPLICATION_DNS_MESSAGE);
    pub const APPLICATION_DOTS_CBOR: Encoding = Encoding::new(prefix::APPLICATION_DOTS_CBOR);
    pub const APPLICATION_DPOP_JWT: Encoding = Encoding::new(prefix::APPLICATION_DPOP_JWT);
    pub const APPLICATION_DSKPP_XML: Encoding = Encoding::new(prefix::APPLICATION_DSKPP_XML);
    pub const APPLICATION_DSSC_DER: Encoding = Encoding::new(prefix::APPLICATION_DSSC_DER);
    pub const APPLICATION_DSSC_XML: Encoding = Encoding::new(prefix::APPLICATION_DSSC_XML);
    pub const APPLICATION_DVCS: Encoding = Encoding::new(prefix::APPLICATION_DVCS);
    pub const APPLICATION_ECMASCRIPT: Encoding = Encoding::new(prefix::APPLICATION_ECMASCRIPT);
    pub const APPLICATION_EDHOC_CBOR_SEQ: Encoding =
        Encoding::new(prefix::APPLICATION_EDHOC_CBOR_SEQ);
    pub const APPLICATION_EFI: Encoding = Encoding::new(prefix::APPLICATION_EFI);
    pub const APPLICATION_ELM_JSON: Encoding = Encoding::new(prefix::APPLICATION_ELM_JSON);
    pub const APPLICATION_ELM_XML: Encoding = Encoding::new(prefix::APPLICATION_ELM_XML);
    pub const APPLICATION_EMMA_XML: Encoding = Encoding::new(prefix::APPLICATION_EMMA_XML);
    pub const APPLICATION_EMOTIONML_XML: Encoding =
        Encoding::new(prefix::APPLICATION_EMOTIONML_XML);
    pub const APPLICATION_ENCAPRTP: Encoding = Encoding::new(prefix::APPLICATION_ENCAPRTP);
    pub const APPLICATION_EPP_XML: Encoding = Encoding::new(prefix::APPLICATION_EPP_XML);
    pub const APPLICATION_EPUB_ZIP: Encoding = Encoding::new(prefix::APPLICATION_EPUB_ZIP);
    pub const APPLICATION_ESHOP: Encoding = Encoding::new(prefix::APPLICATION_ESHOP);
    pub const APPLICATION_EXAMPLE: Encoding = Encoding::new(prefix::APPLICATION_EXAMPLE);
    pub const APPLICATION_EXI: Encoding = Encoding::new(prefix::APPLICATION_EXI);
    pub const APPLICATION_EXPECT_CT_REPORT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_EXPECT_CT_REPORT_JSON);
    pub const APPLICATION_EXPRESS: Encoding = Encoding::new(prefix::APPLICATION_EXPRESS);
    pub const APPLICATION_FASTINFOSET: Encoding = Encoding::new(prefix::APPLICATION_FASTINFOSET);
    pub const APPLICATION_FASTSOAP: Encoding = Encoding::new(prefix::APPLICATION_FASTSOAP);
    pub const APPLICATION_FDF: Encoding = Encoding::new(prefix::APPLICATION_FDF);
    pub const APPLICATION_FDT_XML: Encoding = Encoding::new(prefix::APPLICATION_FDT_XML);
    pub const APPLICATION_FHIR_JSON: Encoding = Encoding::new(prefix::APPLICATION_FHIR_JSON);
    pub const APPLICATION_FHIR_XML: Encoding = Encoding::new(prefix::APPLICATION_FHIR_XML);
    pub const APPLICATION_FITS: Encoding = Encoding::new(prefix::APPLICATION_FITS);
    pub const APPLICATION_FLEXFEC: Encoding = Encoding::new(prefix::APPLICATION_FLEXFEC);
    pub const APPLICATION_FONT_SFNT: Encoding = Encoding::new(prefix::APPLICATION_FONT_SFNT);
    pub const APPLICATION_FONT_TDPFR: Encoding = Encoding::new(prefix::APPLICATION_FONT_TDPFR);
    pub const APPLICATION_FONT_WOFF: Encoding = Encoding::new(prefix::APPLICATION_FONT_WOFF);
    pub const APPLICATION_FRAMEWORK_ATTRIBUTES_XML: Encoding =
        Encoding::new(prefix::APPLICATION_FRAMEWORK_ATTRIBUTES_XML);
    pub const APPLICATION_GEO_JSON: Encoding = Encoding::new(prefix::APPLICATION_GEO_JSON);
    pub const APPLICATION_GEO_JSON_SEQ: Encoding = Encoding::new(prefix::APPLICATION_GEO_JSON_SEQ);
    pub const APPLICATION_GEOPACKAGE_SQLITE3: Encoding =
        Encoding::new(prefix::APPLICATION_GEOPACKAGE_SQLITE3);
    pub const APPLICATION_GEOXACML_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_GEOXACML_JSON);
    pub const APPLICATION_GEOXACML_XML: Encoding = Encoding::new(prefix::APPLICATION_GEOXACML_XML);
    pub const APPLICATION_GLTF_BUFFER: Encoding = Encoding::new(prefix::APPLICATION_GLTF_BUFFER);
    pub const APPLICATION_GML_XML: Encoding = Encoding::new(prefix::APPLICATION_GML_XML);
    pub const APPLICATION_GZIP: Encoding = Encoding::new(prefix::APPLICATION_GZIP);
    pub const APPLICATION_HELD_XML: Encoding = Encoding::new(prefix::APPLICATION_HELD_XML);
    pub const APPLICATION_HL7V2_XML: Encoding = Encoding::new(prefix::APPLICATION_HL7V2_XML);
    pub const APPLICATION_HTTP: Encoding = Encoding::new(prefix::APPLICATION_HTTP);
    pub const APPLICATION_HYPERSTUDIO: Encoding = Encoding::new(prefix::APPLICATION_HYPERSTUDIO);
    pub const APPLICATION_IBE_KEY_REQUEST_XML: Encoding =
        Encoding::new(prefix::APPLICATION_IBE_KEY_REQUEST_XML);
    pub const APPLICATION_IBE_PKG_REPLY_XML: Encoding =
        Encoding::new(prefix::APPLICATION_IBE_PKG_REPLY_XML);
    pub const APPLICATION_IBE_PP_DATA: Encoding = Encoding::new(prefix::APPLICATION_IBE_PP_DATA);
    pub const APPLICATION_IGES: Encoding = Encoding::new(prefix::APPLICATION_IGES);
    pub const APPLICATION_IM_ISCOMPOSING_XML: Encoding =
        Encoding::new(prefix::APPLICATION_IM_ISCOMPOSING_XML);
    pub const APPLICATION_INDEX: Encoding = Encoding::new(prefix::APPLICATION_INDEX);
    pub const APPLICATION_INDEX_CMD: Encoding = Encoding::new(prefix::APPLICATION_INDEX_CMD);
    pub const APPLICATION_INDEX_OBJ: Encoding = Encoding::new(prefix::APPLICATION_INDEX_OBJ);
    pub const APPLICATION_INDEX_RESPONSE: Encoding =
        Encoding::new(prefix::APPLICATION_INDEX_RESPONSE);
    pub const APPLICATION_INDEX_VND: Encoding = Encoding::new(prefix::APPLICATION_INDEX_VND);
    pub const APPLICATION_INKML_XML: Encoding = Encoding::new(prefix::APPLICATION_INKML_XML);
    pub const APPLICATION_IPFIX: Encoding = Encoding::new(prefix::APPLICATION_IPFIX);
    pub const APPLICATION_IPP: Encoding = Encoding::new(prefix::APPLICATION_IPP);
    pub const APPLICATION_ITS_XML: Encoding = Encoding::new(prefix::APPLICATION_ITS_XML);
    pub const APPLICATION_JAVA_ARCHIVE: Encoding = Encoding::new(prefix::APPLICATION_JAVA_ARCHIVE);
    pub const APPLICATION_JAVASCRIPT: Encoding = Encoding::new(prefix::APPLICATION_JAVASCRIPT);
    pub const APPLICATION_JF2FEED_JSON: Encoding = Encoding::new(prefix::APPLICATION_JF2FEED_JSON);
    pub const APPLICATION_JOSE: Encoding = Encoding::new(prefix::APPLICATION_JOSE);
    pub const APPLICATION_JOSE_JSON: Encoding = Encoding::new(prefix::APPLICATION_JOSE_JSON);
    pub const APPLICATION_JRD_JSON: Encoding = Encoding::new(prefix::APPLICATION_JRD_JSON);
    pub const APPLICATION_JSCALENDAR_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_JSCALENDAR_JSON);
    pub const APPLICATION_JSCONTACT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_JSCONTACT_JSON);
    pub const APPLICATION_JSON: Encoding = Encoding::new(prefix::APPLICATION_JSON);
    pub const APPLICATION_JSON_PATCH_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_JSON_PATCH_JSON);
    pub const APPLICATION_JSON_SEQ: Encoding = Encoding::new(prefix::APPLICATION_JSON_SEQ);
    pub const APPLICATION_JSONPATH: Encoding = Encoding::new(prefix::APPLICATION_JSONPATH);
    pub const APPLICATION_JWK_JSON: Encoding = Encoding::new(prefix::APPLICATION_JWK_JSON);
    pub const APPLICATION_JWK_SET_JSON: Encoding = Encoding::new(prefix::APPLICATION_JWK_SET_JSON);
    pub const APPLICATION_JWT: Encoding = Encoding::new(prefix::APPLICATION_JWT);
    pub const APPLICATION_KPML_REQUEST_XML: Encoding =
        Encoding::new(prefix::APPLICATION_KPML_REQUEST_XML);
    pub const APPLICATION_KPML_RESPONSE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_KPML_RESPONSE_XML);
    pub const APPLICATION_LD_JSON: Encoding = Encoding::new(prefix::APPLICATION_LD_JSON);
    pub const APPLICATION_LGR_XML: Encoding = Encoding::new(prefix::APPLICATION_LGR_XML);
    pub const APPLICATION_LINK_FORMAT: Encoding = Encoding::new(prefix::APPLICATION_LINK_FORMAT);
    pub const APPLICATION_LINKSET: Encoding = Encoding::new(prefix::APPLICATION_LINKSET);
    pub const APPLICATION_LINKSET_JSON: Encoding = Encoding::new(prefix::APPLICATION_LINKSET_JSON);
    pub const APPLICATION_LOAD_CONTROL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_LOAD_CONTROL_XML);
    pub const APPLICATION_LOGOUT_JWT: Encoding = Encoding::new(prefix::APPLICATION_LOGOUT_JWT);
    pub const APPLICATION_LOST_XML: Encoding = Encoding::new(prefix::APPLICATION_LOST_XML);
    pub const APPLICATION_LOSTSYNC_XML: Encoding = Encoding::new(prefix::APPLICATION_LOSTSYNC_XML);
    pub const APPLICATION_LPF_ZIP: Encoding = Encoding::new(prefix::APPLICATION_LPF_ZIP);
    pub const APPLICATION_MAC_BINHEX40: Encoding = Encoding::new(prefix::APPLICATION_MAC_BINHEX40);
    pub const APPLICATION_MACWRITEII: Encoding = Encoding::new(prefix::APPLICATION_MACWRITEII);
    pub const APPLICATION_MADS_XML: Encoding = Encoding::new(prefix::APPLICATION_MADS_XML);
    pub const APPLICATION_MANIFEST_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_MANIFEST_JSON);
    pub const APPLICATION_MARC: Encoding = Encoding::new(prefix::APPLICATION_MARC);
    pub const APPLICATION_MARCXML_XML: Encoding = Encoding::new(prefix::APPLICATION_MARCXML_XML);
    pub const APPLICATION_MATHEMATICA: Encoding = Encoding::new(prefix::APPLICATION_MATHEMATICA);
    pub const APPLICATION_MATHML_XML: Encoding = Encoding::new(prefix::APPLICATION_MATHML_XML);
    pub const APPLICATION_MATHML_CONTENT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MATHML_CONTENT_XML);
    pub const APPLICATION_MATHML_PRESENTATION_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MATHML_PRESENTATION_XML);
    pub const APPLICATION_MBMS_ASSOCIATED_PROCEDURE_DESCRIPTION_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MBMS_ASSOCIATED_PROCEDURE_DESCRIPTION_XML);
    pub const APPLICATION_MBMS_DEREGISTER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MBMS_DEREGISTER_XML);
    pub const APPLICATION_MBMS_ENVELOPE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MBMS_ENVELOPE_XML);
    pub const APPLICATION_MBMS_MSK_XML: Encoding = Encoding::new(prefix::APPLICATION_MBMS_MSK_XML);
    pub const APPLICATION_MBMS_MSK_RESPONSE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MBMS_MSK_RESPONSE_XML);
    pub const APPLICATION_MBMS_PROTECTION_DESCRIPTION_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MBMS_PROTECTION_DESCRIPTION_XML);
    pub const APPLICATION_MBMS_RECEPTION_REPORT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MBMS_RECEPTION_REPORT_XML);
    pub const APPLICATION_MBMS_REGISTER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MBMS_REGISTER_XML);
    pub const APPLICATION_MBMS_REGISTER_RESPONSE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MBMS_REGISTER_RESPONSE_XML);
    pub const APPLICATION_MBMS_SCHEDULE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MBMS_SCHEDULE_XML);
    pub const APPLICATION_MBMS_USER_SERVICE_DESCRIPTION_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MBMS_USER_SERVICE_DESCRIPTION_XML);
    pub const APPLICATION_MBOX: Encoding = Encoding::new(prefix::APPLICATION_MBOX);
    pub const APPLICATION_MEDIA_POLICY_DATASET_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MEDIA_POLICY_DATASET_XML);
    pub const APPLICATION_MEDIA_CONTROL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MEDIA_CONTROL_XML);
    pub const APPLICATION_MEDIASERVERCONTROL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MEDIASERVERCONTROL_XML);
    pub const APPLICATION_MERGE_PATCH_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_MERGE_PATCH_JSON);
    pub const APPLICATION_METALINK4_XML: Encoding =
        Encoding::new(prefix::APPLICATION_METALINK4_XML);
    pub const APPLICATION_METS_XML: Encoding = Encoding::new(prefix::APPLICATION_METS_XML);
    pub const APPLICATION_MIKEY: Encoding = Encoding::new(prefix::APPLICATION_MIKEY);
    pub const APPLICATION_MIPC: Encoding = Encoding::new(prefix::APPLICATION_MIPC);
    pub const APPLICATION_MISSING_BLOCKS_CBOR_SEQ: Encoding =
        Encoding::new(prefix::APPLICATION_MISSING_BLOCKS_CBOR_SEQ);
    pub const APPLICATION_MMT_AEI_XML: Encoding = Encoding::new(prefix::APPLICATION_MMT_AEI_XML);
    pub const APPLICATION_MMT_USD_XML: Encoding = Encoding::new(prefix::APPLICATION_MMT_USD_XML);
    pub const APPLICATION_MODS_XML: Encoding = Encoding::new(prefix::APPLICATION_MODS_XML);
    pub const APPLICATION_MOSS_KEYS: Encoding = Encoding::new(prefix::APPLICATION_MOSS_KEYS);
    pub const APPLICATION_MOSS_SIGNATURE: Encoding =
        Encoding::new(prefix::APPLICATION_MOSS_SIGNATURE);
    pub const APPLICATION_MOSSKEY_DATA: Encoding = Encoding::new(prefix::APPLICATION_MOSSKEY_DATA);
    pub const APPLICATION_MOSSKEY_REQUEST: Encoding =
        Encoding::new(prefix::APPLICATION_MOSSKEY_REQUEST);
    pub const APPLICATION_MP21: Encoding = Encoding::new(prefix::APPLICATION_MP21);
    pub const APPLICATION_MP4: Encoding = Encoding::new(prefix::APPLICATION_MP4);
    pub const APPLICATION_MPEG4_GENERIC: Encoding =
        Encoding::new(prefix::APPLICATION_MPEG4_GENERIC);
    pub const APPLICATION_MPEG4_IOD: Encoding = Encoding::new(prefix::APPLICATION_MPEG4_IOD);
    pub const APPLICATION_MPEG4_IOD_XMT: Encoding =
        Encoding::new(prefix::APPLICATION_MPEG4_IOD_XMT);
    pub const APPLICATION_MRB_CONSUMER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MRB_CONSUMER_XML);
    pub const APPLICATION_MRB_PUBLISH_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MRB_PUBLISH_XML);
    pub const APPLICATION_MSC_IVR_XML: Encoding = Encoding::new(prefix::APPLICATION_MSC_IVR_XML);
    pub const APPLICATION_MSC_MIXER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_MSC_MIXER_XML);
    pub const APPLICATION_MSWORD: Encoding = Encoding::new(prefix::APPLICATION_MSWORD);
    pub const APPLICATION_MUD_JSON: Encoding = Encoding::new(prefix::APPLICATION_MUD_JSON);
    pub const APPLICATION_MULTIPART_CORE: Encoding =
        Encoding::new(prefix::APPLICATION_MULTIPART_CORE);
    pub const APPLICATION_MXF: Encoding = Encoding::new(prefix::APPLICATION_MXF);
    pub const APPLICATION_N_QUADS: Encoding = Encoding::new(prefix::APPLICATION_N_QUADS);
    pub const APPLICATION_N_TRIPLES: Encoding = Encoding::new(prefix::APPLICATION_N_TRIPLES);
    pub const APPLICATION_NASDATA: Encoding = Encoding::new(prefix::APPLICATION_NASDATA);
    pub const APPLICATION_NEWS_CHECKGROUPS: Encoding =
        Encoding::new(prefix::APPLICATION_NEWS_CHECKGROUPS);
    pub const APPLICATION_NEWS_GROUPINFO: Encoding =
        Encoding::new(prefix::APPLICATION_NEWS_GROUPINFO);
    pub const APPLICATION_NEWS_TRANSMISSION: Encoding =
        Encoding::new(prefix::APPLICATION_NEWS_TRANSMISSION);
    pub const APPLICATION_NLSML_XML: Encoding = Encoding::new(prefix::APPLICATION_NLSML_XML);
    pub const APPLICATION_NODE: Encoding = Encoding::new(prefix::APPLICATION_NODE);
    pub const APPLICATION_NSS: Encoding = Encoding::new(prefix::APPLICATION_NSS);
    pub const APPLICATION_OAUTH_AUTHZ_REQ_JWT: Encoding =
        Encoding::new(prefix::APPLICATION_OAUTH_AUTHZ_REQ_JWT);
    pub const APPLICATION_OBLIVIOUS_DNS_MESSAGE: Encoding =
        Encoding::new(prefix::APPLICATION_OBLIVIOUS_DNS_MESSAGE);
    pub const APPLICATION_OCSP_REQUEST: Encoding = Encoding::new(prefix::APPLICATION_OCSP_REQUEST);
    pub const APPLICATION_OCSP_RESPONSE: Encoding =
        Encoding::new(prefix::APPLICATION_OCSP_RESPONSE);
    pub const APPLICATION_OCTET_STREAM: Encoding = Encoding::new(prefix::APPLICATION_OCTET_STREAM);
    pub const APPLICATION_ODM_XML: Encoding = Encoding::new(prefix::APPLICATION_ODM_XML);
    pub const APPLICATION_OEBPS_PACKAGE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_OEBPS_PACKAGE_XML);
    pub const APPLICATION_OGG: Encoding = Encoding::new(prefix::APPLICATION_OGG);
    pub const APPLICATION_OHTTP_KEYS: Encoding = Encoding::new(prefix::APPLICATION_OHTTP_KEYS);
    pub const APPLICATION_OPC_NODESET_XML: Encoding =
        Encoding::new(prefix::APPLICATION_OPC_NODESET_XML);
    pub const APPLICATION_OSCORE: Encoding = Encoding::new(prefix::APPLICATION_OSCORE);
    pub const APPLICATION_OXPS: Encoding = Encoding::new(prefix::APPLICATION_OXPS);
    pub const APPLICATION_P21: Encoding = Encoding::new(prefix::APPLICATION_P21);
    pub const APPLICATION_P21_ZIP: Encoding = Encoding::new(prefix::APPLICATION_P21_ZIP);
    pub const APPLICATION_P2P_OVERLAY_XML: Encoding =
        Encoding::new(prefix::APPLICATION_P2P_OVERLAY_XML);
    pub const APPLICATION_PARITYFEC: Encoding = Encoding::new(prefix::APPLICATION_PARITYFEC);
    pub const APPLICATION_PASSPORT: Encoding = Encoding::new(prefix::APPLICATION_PASSPORT);
    pub const APPLICATION_PATCH_OPS_ERROR_XML: Encoding =
        Encoding::new(prefix::APPLICATION_PATCH_OPS_ERROR_XML);
    pub const APPLICATION_PDF: Encoding = Encoding::new(prefix::APPLICATION_PDF);
    pub const APPLICATION_PEM_CERTIFICATE_CHAIN: Encoding =
        Encoding::new(prefix::APPLICATION_PEM_CERTIFICATE_CHAIN);
    pub const APPLICATION_PGP_ENCRYPTED: Encoding =
        Encoding::new(prefix::APPLICATION_PGP_ENCRYPTED);
    pub const APPLICATION_PGP_KEYS: Encoding = Encoding::new(prefix::APPLICATION_PGP_KEYS);
    pub const APPLICATION_PGP_SIGNATURE: Encoding =
        Encoding::new(prefix::APPLICATION_PGP_SIGNATURE);
    pub const APPLICATION_PIDF_XML: Encoding = Encoding::new(prefix::APPLICATION_PIDF_XML);
    pub const APPLICATION_PIDF_DIFF_XML: Encoding =
        Encoding::new(prefix::APPLICATION_PIDF_DIFF_XML);
    pub const APPLICATION_PKCS10: Encoding = Encoding::new(prefix::APPLICATION_PKCS10);
    pub const APPLICATION_PKCS12: Encoding = Encoding::new(prefix::APPLICATION_PKCS12);
    pub const APPLICATION_PKCS7_MIME: Encoding = Encoding::new(prefix::APPLICATION_PKCS7_MIME);
    pub const APPLICATION_PKCS7_SIGNATURE: Encoding =
        Encoding::new(prefix::APPLICATION_PKCS7_SIGNATURE);
    pub const APPLICATION_PKCS8: Encoding = Encoding::new(prefix::APPLICATION_PKCS8);
    pub const APPLICATION_PKCS8_ENCRYPTED: Encoding =
        Encoding::new(prefix::APPLICATION_PKCS8_ENCRYPTED);
    pub const APPLICATION_PKIX_ATTR_CERT: Encoding =
        Encoding::new(prefix::APPLICATION_PKIX_ATTR_CERT);
    pub const APPLICATION_PKIX_CERT: Encoding = Encoding::new(prefix::APPLICATION_PKIX_CERT);
    pub const APPLICATION_PKIX_CRL: Encoding = Encoding::new(prefix::APPLICATION_PKIX_CRL);
    pub const APPLICATION_PKIX_PKIPATH: Encoding = Encoding::new(prefix::APPLICATION_PKIX_PKIPATH);
    pub const APPLICATION_PKIXCMP: Encoding = Encoding::new(prefix::APPLICATION_PKIXCMP);
    pub const APPLICATION_PLS_XML: Encoding = Encoding::new(prefix::APPLICATION_PLS_XML);
    pub const APPLICATION_POC_SETTINGS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_POC_SETTINGS_XML);
    pub const APPLICATION_POSTSCRIPT: Encoding = Encoding::new(prefix::APPLICATION_POSTSCRIPT);
    pub const APPLICATION_PPSP_TRACKER_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_PPSP_TRACKER_JSON);
    pub const APPLICATION_PRIVATE_TOKEN_ISSUER_DIRECTORY: Encoding =
        Encoding::new(prefix::APPLICATION_PRIVATE_TOKEN_ISSUER_DIRECTORY);
    pub const APPLICATION_PRIVATE_TOKEN_REQUEST: Encoding =
        Encoding::new(prefix::APPLICATION_PRIVATE_TOKEN_REQUEST);
    pub const APPLICATION_PRIVATE_TOKEN_RESPONSE: Encoding =
        Encoding::new(prefix::APPLICATION_PRIVATE_TOKEN_RESPONSE);
    pub const APPLICATION_PROBLEM_JSON: Encoding = Encoding::new(prefix::APPLICATION_PROBLEM_JSON);
    pub const APPLICATION_PROBLEM_XML: Encoding = Encoding::new(prefix::APPLICATION_PROBLEM_XML);
    pub const APPLICATION_PROVENANCE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_PROVENANCE_XML);
    pub const APPLICATION_PRS_ALVESTRAND_TITRAX_SHEET: Encoding =
        Encoding::new(prefix::APPLICATION_PRS_ALVESTRAND_TITRAX_SHEET);
    pub const APPLICATION_PRS_CWW: Encoding = Encoding::new(prefix::APPLICATION_PRS_CWW);
    pub const APPLICATION_PRS_CYN: Encoding = Encoding::new(prefix::APPLICATION_PRS_CYN);
    pub const APPLICATION_PRS_HPUB_ZIP: Encoding = Encoding::new(prefix::APPLICATION_PRS_HPUB_ZIP);
    pub const APPLICATION_PRS_IMPLIED_DOCUMENT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_PRS_IMPLIED_DOCUMENT_XML);
    pub const APPLICATION_PRS_IMPLIED_EXECUTABLE: Encoding =
        Encoding::new(prefix::APPLICATION_PRS_IMPLIED_EXECUTABLE);
    pub const APPLICATION_PRS_IMPLIED_OBJECT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_PRS_IMPLIED_OBJECT_JSON);
    pub const APPLICATION_PRS_IMPLIED_OBJECT_JSON_SEQ: Encoding =
        Encoding::new(prefix::APPLICATION_PRS_IMPLIED_OBJECT_JSON_SEQ);
    pub const APPLICATION_PRS_IMPLIED_OBJECT_YAML: Encoding =
        Encoding::new(prefix::APPLICATION_PRS_IMPLIED_OBJECT_YAML);
    pub const APPLICATION_PRS_IMPLIED_STRUCTURE: Encoding =
        Encoding::new(prefix::APPLICATION_PRS_IMPLIED_STRUCTURE);
    pub const APPLICATION_PRS_NPREND: Encoding = Encoding::new(prefix::APPLICATION_PRS_NPREND);
    pub const APPLICATION_PRS_PLUCKER: Encoding = Encoding::new(prefix::APPLICATION_PRS_PLUCKER);
    pub const APPLICATION_PRS_RDF_XML_CRYPT: Encoding =
        Encoding::new(prefix::APPLICATION_PRS_RDF_XML_CRYPT);
    pub const APPLICATION_PRS_VCFBZIP2: Encoding = Encoding::new(prefix::APPLICATION_PRS_VCFBZIP2);
    pub const APPLICATION_PRS_XSF_XML: Encoding = Encoding::new(prefix::APPLICATION_PRS_XSF_XML);
    pub const APPLICATION_PSKC_XML: Encoding = Encoding::new(prefix::APPLICATION_PSKC_XML);
    pub const APPLICATION_PVD_JSON: Encoding = Encoding::new(prefix::APPLICATION_PVD_JSON);
    pub const APPLICATION_RAPTORFEC: Encoding = Encoding::new(prefix::APPLICATION_RAPTORFEC);
    pub const APPLICATION_RDAP_JSON: Encoding = Encoding::new(prefix::APPLICATION_RDAP_JSON);
    pub const APPLICATION_RDF_XML: Encoding = Encoding::new(prefix::APPLICATION_RDF_XML);
    pub const APPLICATION_REGINFO_XML: Encoding = Encoding::new(prefix::APPLICATION_REGINFO_XML);
    pub const APPLICATION_RELAX_NG_COMPACT_SYNTAX: Encoding =
        Encoding::new(prefix::APPLICATION_RELAX_NG_COMPACT_SYNTAX);
    pub const APPLICATION_REMOTE_PRINTING: Encoding =
        Encoding::new(prefix::APPLICATION_REMOTE_PRINTING);
    pub const APPLICATION_REPUTON_JSON: Encoding = Encoding::new(prefix::APPLICATION_REPUTON_JSON);
    pub const APPLICATION_RESOURCE_LISTS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_RESOURCE_LISTS_XML);
    pub const APPLICATION_RESOURCE_LISTS_DIFF_XML: Encoding =
        Encoding::new(prefix::APPLICATION_RESOURCE_LISTS_DIFF_XML);
    pub const APPLICATION_RFC_XML: Encoding = Encoding::new(prefix::APPLICATION_RFC_XML);
    pub const APPLICATION_RISCOS: Encoding = Encoding::new(prefix::APPLICATION_RISCOS);
    pub const APPLICATION_RLMI_XML: Encoding = Encoding::new(prefix::APPLICATION_RLMI_XML);
    pub const APPLICATION_RLS_SERVICES_XML: Encoding =
        Encoding::new(prefix::APPLICATION_RLS_SERVICES_XML);
    pub const APPLICATION_ROUTE_APD_XML: Encoding =
        Encoding::new(prefix::APPLICATION_ROUTE_APD_XML);
    pub const APPLICATION_ROUTE_S_TSID_XML: Encoding =
        Encoding::new(prefix::APPLICATION_ROUTE_S_TSID_XML);
    pub const APPLICATION_ROUTE_USD_XML: Encoding =
        Encoding::new(prefix::APPLICATION_ROUTE_USD_XML);
    pub const APPLICATION_RPKI_CHECKLIST: Encoding =
        Encoding::new(prefix::APPLICATION_RPKI_CHECKLIST);
    pub const APPLICATION_RPKI_GHOSTBUSTERS: Encoding =
        Encoding::new(prefix::APPLICATION_RPKI_GHOSTBUSTERS);
    pub const APPLICATION_RPKI_MANIFEST: Encoding =
        Encoding::new(prefix::APPLICATION_RPKI_MANIFEST);
    pub const APPLICATION_RPKI_PUBLICATION: Encoding =
        Encoding::new(prefix::APPLICATION_RPKI_PUBLICATION);
    pub const APPLICATION_RPKI_ROA: Encoding = Encoding::new(prefix::APPLICATION_RPKI_ROA);
    pub const APPLICATION_RPKI_UPDOWN: Encoding = Encoding::new(prefix::APPLICATION_RPKI_UPDOWN);
    pub const APPLICATION_RTF: Encoding = Encoding::new(prefix::APPLICATION_RTF);
    pub const APPLICATION_RTPLOOPBACK: Encoding = Encoding::new(prefix::APPLICATION_RTPLOOPBACK);
    pub const APPLICATION_RTX: Encoding = Encoding::new(prefix::APPLICATION_RTX);
    pub const APPLICATION_SAMLASSERTION_XML: Encoding =
        Encoding::new(prefix::APPLICATION_SAMLASSERTION_XML);
    pub const APPLICATION_SAMLMETADATA_XML: Encoding =
        Encoding::new(prefix::APPLICATION_SAMLMETADATA_XML);
    pub const APPLICATION_SARIF_JSON: Encoding = Encoding::new(prefix::APPLICATION_SARIF_JSON);
    pub const APPLICATION_SARIF_EXTERNAL_PROPERTIES_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_SARIF_EXTERNAL_PROPERTIES_JSON);
    pub const APPLICATION_SBE: Encoding = Encoding::new(prefix::APPLICATION_SBE);
    pub const APPLICATION_SBML_XML: Encoding = Encoding::new(prefix::APPLICATION_SBML_XML);
    pub const APPLICATION_SCAIP_XML: Encoding = Encoding::new(prefix::APPLICATION_SCAIP_XML);
    pub const APPLICATION_SCIM_JSON: Encoding = Encoding::new(prefix::APPLICATION_SCIM_JSON);
    pub const APPLICATION_SCVP_CV_REQUEST: Encoding =
        Encoding::new(prefix::APPLICATION_SCVP_CV_REQUEST);
    pub const APPLICATION_SCVP_CV_RESPONSE: Encoding =
        Encoding::new(prefix::APPLICATION_SCVP_CV_RESPONSE);
    pub const APPLICATION_SCVP_VP_REQUEST: Encoding =
        Encoding::new(prefix::APPLICATION_SCVP_VP_REQUEST);
    pub const APPLICATION_SCVP_VP_RESPONSE: Encoding =
        Encoding::new(prefix::APPLICATION_SCVP_VP_RESPONSE);
    pub const APPLICATION_SDP: Encoding = Encoding::new(prefix::APPLICATION_SDP);
    pub const APPLICATION_SECEVENT_JWT: Encoding = Encoding::new(prefix::APPLICATION_SECEVENT_JWT);
    pub const APPLICATION_SENML_CBOR: Encoding = Encoding::new(prefix::APPLICATION_SENML_CBOR);
    pub const APPLICATION_SENML_JSON: Encoding = Encoding::new(prefix::APPLICATION_SENML_JSON);
    pub const APPLICATION_SENML_XML: Encoding = Encoding::new(prefix::APPLICATION_SENML_XML);
    pub const APPLICATION_SENML_ETCH_CBOR: Encoding =
        Encoding::new(prefix::APPLICATION_SENML_ETCH_CBOR);
    pub const APPLICATION_SENML_ETCH_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_SENML_ETCH_JSON);
    pub const APPLICATION_SENML_EXI: Encoding = Encoding::new(prefix::APPLICATION_SENML_EXI);
    pub const APPLICATION_SENSML_CBOR: Encoding = Encoding::new(prefix::APPLICATION_SENSML_CBOR);
    pub const APPLICATION_SENSML_JSON: Encoding = Encoding::new(prefix::APPLICATION_SENSML_JSON);
    pub const APPLICATION_SENSML_XML: Encoding = Encoding::new(prefix::APPLICATION_SENSML_XML);
    pub const APPLICATION_SENSML_EXI: Encoding = Encoding::new(prefix::APPLICATION_SENSML_EXI);
    pub const APPLICATION_SEP_XML: Encoding = Encoding::new(prefix::APPLICATION_SEP_XML);
    pub const APPLICATION_SEP_EXI: Encoding = Encoding::new(prefix::APPLICATION_SEP_EXI);
    pub const APPLICATION_SESSION_INFO: Encoding = Encoding::new(prefix::APPLICATION_SESSION_INFO);
    pub const APPLICATION_SET_PAYMENT: Encoding = Encoding::new(prefix::APPLICATION_SET_PAYMENT);
    pub const APPLICATION_SET_PAYMENT_INITIATION: Encoding =
        Encoding::new(prefix::APPLICATION_SET_PAYMENT_INITIATION);
    pub const APPLICATION_SET_REGISTRATION: Encoding =
        Encoding::new(prefix::APPLICATION_SET_REGISTRATION);
    pub const APPLICATION_SET_REGISTRATION_INITIATION: Encoding =
        Encoding::new(prefix::APPLICATION_SET_REGISTRATION_INITIATION);
    pub const APPLICATION_SGML_OPEN_CATALOG: Encoding =
        Encoding::new(prefix::APPLICATION_SGML_OPEN_CATALOG);
    pub const APPLICATION_SHF_XML: Encoding = Encoding::new(prefix::APPLICATION_SHF_XML);
    pub const APPLICATION_SIEVE: Encoding = Encoding::new(prefix::APPLICATION_SIEVE);
    pub const APPLICATION_SIMPLE_FILTER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_SIMPLE_FILTER_XML);
    pub const APPLICATION_SIMPLE_MESSAGE_SUMMARY: Encoding =
        Encoding::new(prefix::APPLICATION_SIMPLE_MESSAGE_SUMMARY);
    pub const APPLICATION_SIMPLESYMBOLCONTAINER: Encoding =
        Encoding::new(prefix::APPLICATION_SIMPLESYMBOLCONTAINER);
    pub const APPLICATION_SIPC: Encoding = Encoding::new(prefix::APPLICATION_SIPC);
    pub const APPLICATION_SLATE: Encoding = Encoding::new(prefix::APPLICATION_SLATE);
    pub const APPLICATION_SMIL: Encoding = Encoding::new(prefix::APPLICATION_SMIL);
    pub const APPLICATION_SMIL_XML: Encoding = Encoding::new(prefix::APPLICATION_SMIL_XML);
    pub const APPLICATION_SMPTE336M: Encoding = Encoding::new(prefix::APPLICATION_SMPTE336M);
    pub const APPLICATION_SOAP_FASTINFOSET: Encoding =
        Encoding::new(prefix::APPLICATION_SOAP_FASTINFOSET);
    pub const APPLICATION_SOAP_XML: Encoding = Encoding::new(prefix::APPLICATION_SOAP_XML);
    pub const APPLICATION_SPARQL_QUERY: Encoding = Encoding::new(prefix::APPLICATION_SPARQL_QUERY);
    pub const APPLICATION_SPARQL_RESULTS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_SPARQL_RESULTS_XML);
    pub const APPLICATION_SPDX_JSON: Encoding = Encoding::new(prefix::APPLICATION_SPDX_JSON);
    pub const APPLICATION_SPIRITS_EVENT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_SPIRITS_EVENT_XML);
    pub const APPLICATION_SQL: Encoding = Encoding::new(prefix::APPLICATION_SQL);
    pub const APPLICATION_SRGS: Encoding = Encoding::new(prefix::APPLICATION_SRGS);
    pub const APPLICATION_SRGS_XML: Encoding = Encoding::new(prefix::APPLICATION_SRGS_XML);
    pub const APPLICATION_SRU_XML: Encoding = Encoding::new(prefix::APPLICATION_SRU_XML);
    pub const APPLICATION_SSML_XML: Encoding = Encoding::new(prefix::APPLICATION_SSML_XML);
    pub const APPLICATION_STIX_JSON: Encoding = Encoding::new(prefix::APPLICATION_STIX_JSON);
    pub const APPLICATION_SWID_CBOR: Encoding = Encoding::new(prefix::APPLICATION_SWID_CBOR);
    pub const APPLICATION_SWID_XML: Encoding = Encoding::new(prefix::APPLICATION_SWID_XML);
    pub const APPLICATION_TAMP_APEX_UPDATE: Encoding =
        Encoding::new(prefix::APPLICATION_TAMP_APEX_UPDATE);
    pub const APPLICATION_TAMP_APEX_UPDATE_CONFIRM: Encoding =
        Encoding::new(prefix::APPLICATION_TAMP_APEX_UPDATE_CONFIRM);
    pub const APPLICATION_TAMP_COMMUNITY_UPDATE: Encoding =
        Encoding::new(prefix::APPLICATION_TAMP_COMMUNITY_UPDATE);
    pub const APPLICATION_TAMP_COMMUNITY_UPDATE_CONFIRM: Encoding =
        Encoding::new(prefix::APPLICATION_TAMP_COMMUNITY_UPDATE_CONFIRM);
    pub const APPLICATION_TAMP_ERROR: Encoding = Encoding::new(prefix::APPLICATION_TAMP_ERROR);
    pub const APPLICATION_TAMP_SEQUENCE_ADJUST: Encoding =
        Encoding::new(prefix::APPLICATION_TAMP_SEQUENCE_ADJUST);
    pub const APPLICATION_TAMP_SEQUENCE_ADJUST_CONFIRM: Encoding =
        Encoding::new(prefix::APPLICATION_TAMP_SEQUENCE_ADJUST_CONFIRM);
    pub const APPLICATION_TAMP_STATUS_QUERY: Encoding =
        Encoding::new(prefix::APPLICATION_TAMP_STATUS_QUERY);
    pub const APPLICATION_TAMP_STATUS_RESPONSE: Encoding =
        Encoding::new(prefix::APPLICATION_TAMP_STATUS_RESPONSE);
    pub const APPLICATION_TAMP_UPDATE: Encoding = Encoding::new(prefix::APPLICATION_TAMP_UPDATE);
    pub const APPLICATION_TAMP_UPDATE_CONFIRM: Encoding =
        Encoding::new(prefix::APPLICATION_TAMP_UPDATE_CONFIRM);
    pub const APPLICATION_TAXII_JSON: Encoding = Encoding::new(prefix::APPLICATION_TAXII_JSON);
    pub const APPLICATION_TD_JSON: Encoding = Encoding::new(prefix::APPLICATION_TD_JSON);
    pub const APPLICATION_TEI_XML: Encoding = Encoding::new(prefix::APPLICATION_TEI_XML);
    pub const APPLICATION_THRAUD_XML: Encoding = Encoding::new(prefix::APPLICATION_THRAUD_XML);
    pub const APPLICATION_TIMESTAMP_QUERY: Encoding =
        Encoding::new(prefix::APPLICATION_TIMESTAMP_QUERY);
    pub const APPLICATION_TIMESTAMP_REPLY: Encoding =
        Encoding::new(prefix::APPLICATION_TIMESTAMP_REPLY);
    pub const APPLICATION_TIMESTAMPED_DATA: Encoding =
        Encoding::new(prefix::APPLICATION_TIMESTAMPED_DATA);
    pub const APPLICATION_TLSRPT_GZIP: Encoding = Encoding::new(prefix::APPLICATION_TLSRPT_GZIP);
    pub const APPLICATION_TLSRPT_JSON: Encoding = Encoding::new(prefix::APPLICATION_TLSRPT_JSON);
    pub const APPLICATION_TM_JSON: Encoding = Encoding::new(prefix::APPLICATION_TM_JSON);
    pub const APPLICATION_TNAUTHLIST: Encoding = Encoding::new(prefix::APPLICATION_TNAUTHLIST);
    pub const APPLICATION_TOKEN_INTROSPECTION_JWT: Encoding =
        Encoding::new(prefix::APPLICATION_TOKEN_INTROSPECTION_JWT);
    pub const APPLICATION_TRICKLE_ICE_SDPFRAG: Encoding =
        Encoding::new(prefix::APPLICATION_TRICKLE_ICE_SDPFRAG);
    pub const APPLICATION_TRIG: Encoding = Encoding::new(prefix::APPLICATION_TRIG);
    pub const APPLICATION_TTML_XML: Encoding = Encoding::new(prefix::APPLICATION_TTML_XML);
    pub const APPLICATION_TVE_TRIGGER: Encoding = Encoding::new(prefix::APPLICATION_TVE_TRIGGER);
    pub const APPLICATION_TZIF: Encoding = Encoding::new(prefix::APPLICATION_TZIF);
    pub const APPLICATION_TZIF_LEAP: Encoding = Encoding::new(prefix::APPLICATION_TZIF_LEAP);
    pub const APPLICATION_ULPFEC: Encoding = Encoding::new(prefix::APPLICATION_ULPFEC);
    pub const APPLICATION_URC_GRPSHEET_XML: Encoding =
        Encoding::new(prefix::APPLICATION_URC_GRPSHEET_XML);
    pub const APPLICATION_URC_RESSHEET_XML: Encoding =
        Encoding::new(prefix::APPLICATION_URC_RESSHEET_XML);
    pub const APPLICATION_URC_TARGETDESC_XML: Encoding =
        Encoding::new(prefix::APPLICATION_URC_TARGETDESC_XML);
    pub const APPLICATION_URC_UISOCKETDESC_XML: Encoding =
        Encoding::new(prefix::APPLICATION_URC_UISOCKETDESC_XML);
    pub const APPLICATION_VCARD_JSON: Encoding = Encoding::new(prefix::APPLICATION_VCARD_JSON);
    pub const APPLICATION_VCARD_XML: Encoding = Encoding::new(prefix::APPLICATION_VCARD_XML);
    pub const APPLICATION_VEMMI: Encoding = Encoding::new(prefix::APPLICATION_VEMMI);
    pub const APPLICATION_VND_1000MINDS_DECISION_MODEL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_1000MINDS_DECISION_MODEL_XML);
    pub const APPLICATION_VND_1OB: Encoding = Encoding::new(prefix::APPLICATION_VND_1OB);
    pub const APPLICATION_VND_3M_POST_IT_NOTES: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3M_POST_IT_NOTES);
    pub const APPLICATION_VND_3GPP_PROSE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_PROSE_XML);
    pub const APPLICATION_VND_3GPP_PROSE_PC3A_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_PROSE_PC3A_XML);
    pub const APPLICATION_VND_3GPP_PROSE_PC3ACH_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_PROSE_PC3ACH_XML);
    pub const APPLICATION_VND_3GPP_PROSE_PC3CH_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_PROSE_PC3CH_XML);
    pub const APPLICATION_VND_3GPP_PROSE_PC8_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_PROSE_PC8_XML);
    pub const APPLICATION_VND_3GPP_V2X_LOCAL_SERVICE_INFORMATION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_V2X_LOCAL_SERVICE_INFORMATION);
    pub const APPLICATION_VND_3GPP_5GNAS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_5GNAS);
    pub const APPLICATION_VND_3GPP_GMOP_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_GMOP_XML);
    pub const APPLICATION_VND_3GPP_SRVCC_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_SRVCC_INFO_XML);
    pub const APPLICATION_VND_3GPP_ACCESS_TRANSFER_EVENTS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_ACCESS_TRANSFER_EVENTS_XML);
    pub const APPLICATION_VND_3GPP_BSF_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_BSF_XML);
    pub const APPLICATION_VND_3GPP_CRS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_CRS_XML);
    pub const APPLICATION_VND_3GPP_CURRENT_LOCATION_DISCOVERY_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_CURRENT_LOCATION_DISCOVERY_XML);
    pub const APPLICATION_VND_3GPP_GTPC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_GTPC);
    pub const APPLICATION_VND_3GPP_INTERWORKING_DATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_INTERWORKING_DATA);
    pub const APPLICATION_VND_3GPP_LPP: Encoding = Encoding::new(prefix::APPLICATION_VND_3GPP_LPP);
    pub const APPLICATION_VND_3GPP_MC_SIGNALLING_EAR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MC_SIGNALLING_EAR);
    pub const APPLICATION_VND_3GPP_MCDATA_AFFILIATION_COMMAND_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCDATA_AFFILIATION_COMMAND_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCDATA_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_MSGSTORE_CTRL_REQUEST_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCDATA_MSGSTORE_CTRL_REQUEST_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_PAYLOAD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCDATA_PAYLOAD);
    pub const APPLICATION_VND_3GPP_MCDATA_REGROUP_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCDATA_REGROUP_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_SERVICE_CONFIG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCDATA_SERVICE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_SIGNALLING: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCDATA_SIGNALLING);
    pub const APPLICATION_VND_3GPP_MCDATA_UE_CONFIG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCDATA_UE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_USER_PROFILE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCDATA_USER_PROFILE_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_AFFILIATION_COMMAND_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCPTT_AFFILIATION_COMMAND_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_FLOOR_REQUEST_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCPTT_FLOOR_REQUEST_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCPTT_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_LOCATION_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCPTT_LOCATION_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_MBMS_USAGE_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCPTT_MBMS_USAGE_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_REGROUP_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCPTT_REGROUP_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_SERVICE_CONFIG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCPTT_SERVICE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_SIGNED_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCPTT_SIGNED_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_UE_CONFIG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCPTT_UE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_UE_INIT_CONFIG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCPTT_UE_INIT_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_USER_PROFILE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCPTT_USER_PROFILE_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_COMMAND_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_COMMAND_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCVIDEO_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_LOCATION_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCVIDEO_LOCATION_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_MBMS_USAGE_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCVIDEO_MBMS_USAGE_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_REGROUP_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCVIDEO_REGROUP_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_SERVICE_CONFIG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCVIDEO_SERVICE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_TRANSMISSION_REQUEST_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCVIDEO_TRANSMISSION_REQUEST_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_UE_CONFIG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCVIDEO_UE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_USER_PROFILE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MCVIDEO_USER_PROFILE_XML);
    pub const APPLICATION_VND_3GPP_MID_CALL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_MID_CALL_XML);
    pub const APPLICATION_VND_3GPP_NGAP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_NGAP);
    pub const APPLICATION_VND_3GPP_PFCP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_PFCP);
    pub const APPLICATION_VND_3GPP_PIC_BW_LARGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_PIC_BW_LARGE);
    pub const APPLICATION_VND_3GPP_PIC_BW_SMALL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_PIC_BW_SMALL);
    pub const APPLICATION_VND_3GPP_PIC_BW_VAR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_PIC_BW_VAR);
    pub const APPLICATION_VND_3GPP_S1AP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_S1AP);
    pub const APPLICATION_VND_3GPP_SEAL_GROUP_DOC_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_SEAL_GROUP_DOC_XML);
    pub const APPLICATION_VND_3GPP_SEAL_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_SEAL_INFO_XML);
    pub const APPLICATION_VND_3GPP_SEAL_LOCATION_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_SEAL_LOCATION_INFO_XML);
    pub const APPLICATION_VND_3GPP_SEAL_MBMS_USAGE_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_SEAL_MBMS_USAGE_INFO_XML);
    pub const APPLICATION_VND_3GPP_SEAL_NETWORK_QOS_MANAGEMENT_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_SEAL_NETWORK_QOS_MANAGEMENT_INFO_XML);
    pub const APPLICATION_VND_3GPP_SEAL_UE_CONFIG_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_SEAL_UE_CONFIG_INFO_XML);
    pub const APPLICATION_VND_3GPP_SEAL_UNICAST_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_SEAL_UNICAST_INFO_XML);
    pub const APPLICATION_VND_3GPP_SEAL_USER_PROFILE_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_SEAL_USER_PROFILE_INFO_XML);
    pub const APPLICATION_VND_3GPP_SMS: Encoding = Encoding::new(prefix::APPLICATION_VND_3GPP_SMS);
    pub const APPLICATION_VND_3GPP_SMS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_SMS_XML);
    pub const APPLICATION_VND_3GPP_SRVCC_EXT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_SRVCC_EXT_XML);
    pub const APPLICATION_VND_3GPP_STATE_AND_EVENT_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_STATE_AND_EVENT_INFO_XML);
    pub const APPLICATION_VND_3GPP_USSD_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_USSD_XML);
    pub const APPLICATION_VND_3GPP_V2X: Encoding = Encoding::new(prefix::APPLICATION_VND_3GPP_V2X);
    pub const APPLICATION_VND_3GPP_VAE_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP_VAE_INFO_XML);
    pub const APPLICATION_VND_3GPP2_BCMCSINFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP2_BCMCSINFO_XML);
    pub const APPLICATION_VND_3GPP2_SMS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP2_SMS);
    pub const APPLICATION_VND_3GPP2_TCAP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3GPP2_TCAP);
    pub const APPLICATION_VND_3LIGHTSSOFTWARE_IMAGESCAL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_3LIGHTSSOFTWARE_IMAGESCAL);
    pub const APPLICATION_VND_FLOGRAPHIT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FLOGRAPHIT);
    pub const APPLICATION_VND_HANDHELD_ENTERTAINMENT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_HANDHELD_ENTERTAINMENT_XML);
    pub const APPLICATION_VND_KINAR: Encoding = Encoding::new(prefix::APPLICATION_VND_KINAR);
    pub const APPLICATION_VND_MFER: Encoding = Encoding::new(prefix::APPLICATION_VND_MFER);
    pub const APPLICATION_VND_MOBIUS_DAF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOBIUS_DAF);
    pub const APPLICATION_VND_MOBIUS_DIS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOBIUS_DIS);
    pub const APPLICATION_VND_MOBIUS_MBK: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOBIUS_MBK);
    pub const APPLICATION_VND_MOBIUS_MQY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOBIUS_MQY);
    pub const APPLICATION_VND_MOBIUS_MSL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOBIUS_MSL);
    pub const APPLICATION_VND_MOBIUS_PLC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOBIUS_PLC);
    pub const APPLICATION_VND_MOBIUS_TXF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOBIUS_TXF);
    pub const APPLICATION_VND_QUARK_QUARKXPRESS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_QUARK_QUARKXPRESS);
    pub const APPLICATION_VND_RENLEARN_RLPRINT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RENLEARN_RLPRINT);
    pub const APPLICATION_VND_SIMTECH_MINDMAPPER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SIMTECH_MINDMAPPER);
    pub const APPLICATION_VND_ACCPAC_SIMPLY_ASO: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ACCPAC_SIMPLY_ASO);
    pub const APPLICATION_VND_ACCPAC_SIMPLY_IMP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ACCPAC_SIMPLY_IMP);
    pub const APPLICATION_VND_ACM_ADDRESSXFER_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ACM_ADDRESSXFER_JSON);
    pub const APPLICATION_VND_ACM_CHATBOT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ACM_CHATBOT_JSON);
    pub const APPLICATION_VND_ACUCOBOL: Encoding = Encoding::new(prefix::APPLICATION_VND_ACUCOBOL);
    pub const APPLICATION_VND_ACUCORP: Encoding = Encoding::new(prefix::APPLICATION_VND_ACUCORP);
    pub const APPLICATION_VND_ADOBE_FLASH_MOVIE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ADOBE_FLASH_MOVIE);
    pub const APPLICATION_VND_ADOBE_FORMSCENTRAL_FCDT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ADOBE_FORMSCENTRAL_FCDT);
    pub const APPLICATION_VND_ADOBE_FXP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ADOBE_FXP);
    pub const APPLICATION_VND_ADOBE_PARTIAL_UPLOAD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ADOBE_PARTIAL_UPLOAD);
    pub const APPLICATION_VND_ADOBE_XDP_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ADOBE_XDP_XML);
    pub const APPLICATION_VND_AETHER_IMP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AETHER_IMP);
    pub const APPLICATION_VND_AFPC_AFPLINEDATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_AFPLINEDATA);
    pub const APPLICATION_VND_AFPC_AFPLINEDATA_PAGEDEF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_AFPLINEDATA_PAGEDEF);
    pub const APPLICATION_VND_AFPC_CMOCA_CMRESOURCE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_CMOCA_CMRESOURCE);
    pub const APPLICATION_VND_AFPC_FOCA_CHARSET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_FOCA_CHARSET);
    pub const APPLICATION_VND_AFPC_FOCA_CODEDFONT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_FOCA_CODEDFONT);
    pub const APPLICATION_VND_AFPC_FOCA_CODEPAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_FOCA_CODEPAGE);
    pub const APPLICATION_VND_AFPC_MODCA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_MODCA);
    pub const APPLICATION_VND_AFPC_MODCA_CMTABLE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_MODCA_CMTABLE);
    pub const APPLICATION_VND_AFPC_MODCA_FORMDEF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_MODCA_FORMDEF);
    pub const APPLICATION_VND_AFPC_MODCA_MEDIUMMAP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_MODCA_MEDIUMMAP);
    pub const APPLICATION_VND_AFPC_MODCA_OBJECTCONTAINER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_MODCA_OBJECTCONTAINER);
    pub const APPLICATION_VND_AFPC_MODCA_OVERLAY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_MODCA_OVERLAY);
    pub const APPLICATION_VND_AFPC_MODCA_PAGESEGMENT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AFPC_MODCA_PAGESEGMENT);
    pub const APPLICATION_VND_AGE: Encoding = Encoding::new(prefix::APPLICATION_VND_AGE);
    pub const APPLICATION_VND_AH_BARCODE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AH_BARCODE);
    pub const APPLICATION_VND_AHEAD_SPACE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AHEAD_SPACE);
    pub const APPLICATION_VND_AIRZIP_FILESECURE_AZF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AIRZIP_FILESECURE_AZF);
    pub const APPLICATION_VND_AIRZIP_FILESECURE_AZS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AIRZIP_FILESECURE_AZS);
    pub const APPLICATION_VND_AMADEUS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AMADEUS_JSON);
    pub const APPLICATION_VND_AMAZON_MOBI8_EBOOK: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AMAZON_MOBI8_EBOOK);
    pub const APPLICATION_VND_AMERICANDYNAMICS_ACC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AMERICANDYNAMICS_ACC);
    pub const APPLICATION_VND_AMIGA_AMI: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AMIGA_AMI);
    pub const APPLICATION_VND_AMUNDSEN_MAZE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AMUNDSEN_MAZE_XML);
    pub const APPLICATION_VND_ANDROID_OTA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ANDROID_OTA);
    pub const APPLICATION_VND_ANKI: Encoding = Encoding::new(prefix::APPLICATION_VND_ANKI);
    pub const APPLICATION_VND_ANSER_WEB_CERTIFICATE_ISSUE_INITIATION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ANSER_WEB_CERTIFICATE_ISSUE_INITIATION);
    pub const APPLICATION_VND_ANTIX_GAME_COMPONENT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ANTIX_GAME_COMPONENT);
    pub const APPLICATION_VND_APACHE_ARROW_FILE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APACHE_ARROW_FILE);
    pub const APPLICATION_VND_APACHE_ARROW_STREAM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APACHE_ARROW_STREAM);
    pub const APPLICATION_VND_APACHE_PARQUET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APACHE_PARQUET);
    pub const APPLICATION_VND_APACHE_THRIFT_BINARY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APACHE_THRIFT_BINARY);
    pub const APPLICATION_VND_APACHE_THRIFT_COMPACT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APACHE_THRIFT_COMPACT);
    pub const APPLICATION_VND_APACHE_THRIFT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APACHE_THRIFT_JSON);
    pub const APPLICATION_VND_APEXLANG: Encoding = Encoding::new(prefix::APPLICATION_VND_APEXLANG);
    pub const APPLICATION_VND_API_JSON: Encoding = Encoding::new(prefix::APPLICATION_VND_API_JSON);
    pub const APPLICATION_VND_APLEXTOR_WARRP_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APLEXTOR_WARRP_JSON);
    pub const APPLICATION_VND_APOTHEKENDE_RESERVATION_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APOTHEKENDE_RESERVATION_JSON);
    pub const APPLICATION_VND_APPLE_INSTALLER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APPLE_INSTALLER_XML);
    pub const APPLICATION_VND_APPLE_KEYNOTE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APPLE_KEYNOTE);
    pub const APPLICATION_VND_APPLE_MPEGURL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APPLE_MPEGURL);
    pub const APPLICATION_VND_APPLE_NUMBERS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APPLE_NUMBERS);
    pub const APPLICATION_VND_APPLE_PAGES: Encoding =
        Encoding::new(prefix::APPLICATION_VND_APPLE_PAGES);
    pub const APPLICATION_VND_ARASTRA_SWI: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ARASTRA_SWI);
    pub const APPLICATION_VND_ARISTANETWORKS_SWI: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ARISTANETWORKS_SWI);
    pub const APPLICATION_VND_ARTISAN_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ARTISAN_JSON);
    pub const APPLICATION_VND_ARTSQUARE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ARTSQUARE);
    pub const APPLICATION_VND_ASTRAEA_SOFTWARE_IOTA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ASTRAEA_SOFTWARE_IOTA);
    pub const APPLICATION_VND_AUDIOGRAPH: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AUDIOGRAPH);
    pub const APPLICATION_VND_AUTOPACKAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AUTOPACKAGE);
    pub const APPLICATION_VND_AVALON_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AVALON_JSON);
    pub const APPLICATION_VND_AVISTAR_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_AVISTAR_XML);
    pub const APPLICATION_VND_BALSAMIQ_BMML_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BALSAMIQ_BMML_XML);
    pub const APPLICATION_VND_BALSAMIQ_BMPR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BALSAMIQ_BMPR);
    pub const APPLICATION_VND_BANANA_ACCOUNTING: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BANANA_ACCOUNTING);
    pub const APPLICATION_VND_BBF_USP_ERROR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BBF_USP_ERROR);
    pub const APPLICATION_VND_BBF_USP_MSG: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BBF_USP_MSG);
    pub const APPLICATION_VND_BBF_USP_MSG_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BBF_USP_MSG_JSON);
    pub const APPLICATION_VND_BEKITZUR_STECH_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BEKITZUR_STECH_JSON);
    pub const APPLICATION_VND_BELIGHTSOFT_LHZD_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BELIGHTSOFT_LHZD_ZIP);
    pub const APPLICATION_VND_BELIGHTSOFT_LHZL_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BELIGHTSOFT_LHZL_ZIP);
    pub const APPLICATION_VND_BINT_MED_CONTENT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BINT_MED_CONTENT);
    pub const APPLICATION_VND_BIOPAX_RDF_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BIOPAX_RDF_XML);
    pub const APPLICATION_VND_BLINK_IDB_VALUE_WRAPPER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BLINK_IDB_VALUE_WRAPPER);
    pub const APPLICATION_VND_BLUEICE_MULTIPASS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BLUEICE_MULTIPASS);
    pub const APPLICATION_VND_BLUETOOTH_EP_OOB: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BLUETOOTH_EP_OOB);
    pub const APPLICATION_VND_BLUETOOTH_LE_OOB: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BLUETOOTH_LE_OOB);
    pub const APPLICATION_VND_BMI: Encoding = Encoding::new(prefix::APPLICATION_VND_BMI);
    pub const APPLICATION_VND_BPF: Encoding = Encoding::new(prefix::APPLICATION_VND_BPF);
    pub const APPLICATION_VND_BPF3: Encoding = Encoding::new(prefix::APPLICATION_VND_BPF3);
    pub const APPLICATION_VND_BUSINESSOBJECTS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BUSINESSOBJECTS);
    pub const APPLICATION_VND_BYU_UAPI_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_BYU_UAPI_JSON);
    pub const APPLICATION_VND_BZIP3: Encoding = Encoding::new(prefix::APPLICATION_VND_BZIP3);
    pub const APPLICATION_VND_CAB_JSCRIPT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CAB_JSCRIPT);
    pub const APPLICATION_VND_CANON_CPDL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CANON_CPDL);
    pub const APPLICATION_VND_CANON_LIPS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CANON_LIPS);
    pub const APPLICATION_VND_CAPASYSTEMS_PG_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CAPASYSTEMS_PG_JSON);
    pub const APPLICATION_VND_CENDIO_THINLINC_CLIENTCONF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CENDIO_THINLINC_CLIENTCONF);
    pub const APPLICATION_VND_CENTURY_SYSTEMS_TCP_STREAM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CENTURY_SYSTEMS_TCP_STREAM);
    pub const APPLICATION_VND_CHEMDRAW_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CHEMDRAW_XML);
    pub const APPLICATION_VND_CHESS_PGN: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CHESS_PGN);
    pub const APPLICATION_VND_CHIPNUTS_KARAOKE_MMD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CHIPNUTS_KARAOKE_MMD);
    pub const APPLICATION_VND_CIEDI: Encoding = Encoding::new(prefix::APPLICATION_VND_CIEDI);
    pub const APPLICATION_VND_CINDERELLA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CINDERELLA);
    pub const APPLICATION_VND_CIRPACK_ISDN_EXT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CIRPACK_ISDN_EXT);
    pub const APPLICATION_VND_CITATIONSTYLES_STYLE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CITATIONSTYLES_STYLE_XML);
    pub const APPLICATION_VND_CLAYMORE: Encoding = Encoding::new(prefix::APPLICATION_VND_CLAYMORE);
    pub const APPLICATION_VND_CLOANTO_RP9: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CLOANTO_RP9);
    pub const APPLICATION_VND_CLONK_C4GROUP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CLONK_C4GROUP);
    pub const APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG);
    pub const APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG_PKG: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG_PKG);
    pub const APPLICATION_VND_CNCF_HELM_CHART_CONTENT_V1_TAR_GZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CNCF_HELM_CHART_CONTENT_V1_TAR_GZIP);
    pub const APPLICATION_VND_CNCF_HELM_CHART_PROVENANCE_V1_PROV: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CNCF_HELM_CHART_PROVENANCE_V1_PROV);
    pub const APPLICATION_VND_CNCF_HELM_CONFIG_V1_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CNCF_HELM_CONFIG_V1_JSON);
    pub const APPLICATION_VND_COFFEESCRIPT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COFFEESCRIPT);
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT);
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT_TEMPLATE);
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION);
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION_TEMPLATE);
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET);
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET_TEMPLATE);
    pub const APPLICATION_VND_COLLECTION_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COLLECTION_JSON);
    pub const APPLICATION_VND_COLLECTION_DOC_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COLLECTION_DOC_JSON);
    pub const APPLICATION_VND_COLLECTION_NEXT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COLLECTION_NEXT_JSON);
    pub const APPLICATION_VND_COMICBOOK_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COMICBOOK_ZIP);
    pub const APPLICATION_VND_COMICBOOK_RAR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COMICBOOK_RAR);
    pub const APPLICATION_VND_COMMERCE_BATTELLE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COMMERCE_BATTELLE);
    pub const APPLICATION_VND_COMMONSPACE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COMMONSPACE);
    pub const APPLICATION_VND_CONTACT_CMSG: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CONTACT_CMSG);
    pub const APPLICATION_VND_COREOS_IGNITION_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COREOS_IGNITION_JSON);
    pub const APPLICATION_VND_COSMOCALLER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_COSMOCALLER);
    pub const APPLICATION_VND_CRICK_CLICKER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CRICK_CLICKER);
    pub const APPLICATION_VND_CRICK_CLICKER_KEYBOARD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CRICK_CLICKER_KEYBOARD);
    pub const APPLICATION_VND_CRICK_CLICKER_PALETTE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CRICK_CLICKER_PALETTE);
    pub const APPLICATION_VND_CRICK_CLICKER_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CRICK_CLICKER_TEMPLATE);
    pub const APPLICATION_VND_CRICK_CLICKER_WORDBANK: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CRICK_CLICKER_WORDBANK);
    pub const APPLICATION_VND_CRITICALTOOLS_WBS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CRITICALTOOLS_WBS_XML);
    pub const APPLICATION_VND_CRYPTII_PIPE_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CRYPTII_PIPE_JSON);
    pub const APPLICATION_VND_CRYPTO_SHADE_FILE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CRYPTO_SHADE_FILE);
    pub const APPLICATION_VND_CRYPTOMATOR_ENCRYPTED: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CRYPTOMATOR_ENCRYPTED);
    pub const APPLICATION_VND_CRYPTOMATOR_VAULT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CRYPTOMATOR_VAULT);
    pub const APPLICATION_VND_CTC_POSML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CTC_POSML);
    pub const APPLICATION_VND_CTCT_WS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CTCT_WS_XML);
    pub const APPLICATION_VND_CUPS_PDF: Encoding = Encoding::new(prefix::APPLICATION_VND_CUPS_PDF);
    pub const APPLICATION_VND_CUPS_POSTSCRIPT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CUPS_POSTSCRIPT);
    pub const APPLICATION_VND_CUPS_PPD: Encoding = Encoding::new(prefix::APPLICATION_VND_CUPS_PPD);
    pub const APPLICATION_VND_CUPS_RASTER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CUPS_RASTER);
    pub const APPLICATION_VND_CUPS_RAW: Encoding = Encoding::new(prefix::APPLICATION_VND_CUPS_RAW);
    pub const APPLICATION_VND_CURL: Encoding = Encoding::new(prefix::APPLICATION_VND_CURL);
    pub const APPLICATION_VND_CYAN_DEAN_ROOT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CYAN_DEAN_ROOT_XML);
    pub const APPLICATION_VND_CYBANK: Encoding = Encoding::new(prefix::APPLICATION_VND_CYBANK);
    pub const APPLICATION_VND_CYCLONEDX_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CYCLONEDX_JSON);
    pub const APPLICATION_VND_CYCLONEDX_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_CYCLONEDX_XML);
    pub const APPLICATION_VND_D2L_COURSEPACKAGE1P0_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_D2L_COURSEPACKAGE1P0_ZIP);
    pub const APPLICATION_VND_D3M_DATASET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_D3M_DATASET);
    pub const APPLICATION_VND_D3M_PROBLEM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_D3M_PROBLEM);
    pub const APPLICATION_VND_DART: Encoding = Encoding::new(prefix::APPLICATION_VND_DART);
    pub const APPLICATION_VND_DATA_VISION_RDZ: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DATA_VISION_RDZ);
    pub const APPLICATION_VND_DATALOG: Encoding = Encoding::new(prefix::APPLICATION_VND_DATALOG);
    pub const APPLICATION_VND_DATAPACKAGE_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DATAPACKAGE_JSON);
    pub const APPLICATION_VND_DATARESOURCE_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DATARESOURCE_JSON);
    pub const APPLICATION_VND_DBF: Encoding = Encoding::new(prefix::APPLICATION_VND_DBF);
    pub const APPLICATION_VND_DEBIAN_BINARY_PACKAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DEBIAN_BINARY_PACKAGE);
    pub const APPLICATION_VND_DECE_DATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DECE_DATA);
    pub const APPLICATION_VND_DECE_TTML_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DECE_TTML_XML);
    pub const APPLICATION_VND_DECE_UNSPECIFIED: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DECE_UNSPECIFIED);
    pub const APPLICATION_VND_DECE_ZIP: Encoding = Encoding::new(prefix::APPLICATION_VND_DECE_ZIP);
    pub const APPLICATION_VND_DENOVO_FCSELAYOUT_LINK: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DENOVO_FCSELAYOUT_LINK);
    pub const APPLICATION_VND_DESMUME_MOVIE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DESMUME_MOVIE);
    pub const APPLICATION_VND_DIR_BI_PLATE_DL_NOSUFFIX: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DIR_BI_PLATE_DL_NOSUFFIX);
    pub const APPLICATION_VND_DM_DELEGATION_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DM_DELEGATION_XML);
    pub const APPLICATION_VND_DNA: Encoding = Encoding::new(prefix::APPLICATION_VND_DNA);
    pub const APPLICATION_VND_DOCUMENT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DOCUMENT_JSON);
    pub const APPLICATION_VND_DOLBY_MOBILE_1: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DOLBY_MOBILE_1);
    pub const APPLICATION_VND_DOLBY_MOBILE_2: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DOLBY_MOBILE_2);
    pub const APPLICATION_VND_DOREMIR_SCORECLOUD_BINARY_DOCUMENT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DOREMIR_SCORECLOUD_BINARY_DOCUMENT);
    pub const APPLICATION_VND_DPGRAPH: Encoding = Encoding::new(prefix::APPLICATION_VND_DPGRAPH);
    pub const APPLICATION_VND_DREAMFACTORY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DREAMFACTORY);
    pub const APPLICATION_VND_DRIVE_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DRIVE_JSON);
    pub const APPLICATION_VND_DTG_LOCAL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DTG_LOCAL);
    pub const APPLICATION_VND_DTG_LOCAL_FLASH: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DTG_LOCAL_FLASH);
    pub const APPLICATION_VND_DTG_LOCAL_HTML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DTG_LOCAL_HTML);
    pub const APPLICATION_VND_DVB_AIT: Encoding = Encoding::new(prefix::APPLICATION_VND_DVB_AIT);
    pub const APPLICATION_VND_DVB_DVBISL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_DVBISL_XML);
    pub const APPLICATION_VND_DVB_DVBJ: Encoding = Encoding::new(prefix::APPLICATION_VND_DVB_DVBJ);
    pub const APPLICATION_VND_DVB_ESGCONTAINER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_ESGCONTAINER);
    pub const APPLICATION_VND_DVB_IPDCDFTNOTIFACCESS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_IPDCDFTNOTIFACCESS);
    pub const APPLICATION_VND_DVB_IPDCESGACCESS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_IPDCESGACCESS);
    pub const APPLICATION_VND_DVB_IPDCESGACCESS2: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_IPDCESGACCESS2);
    pub const APPLICATION_VND_DVB_IPDCESGPDD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_IPDCESGPDD);
    pub const APPLICATION_VND_DVB_IPDCROAMING: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_IPDCROAMING);
    pub const APPLICATION_VND_DVB_IPTV_ALFEC_BASE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_IPTV_ALFEC_BASE);
    pub const APPLICATION_VND_DVB_IPTV_ALFEC_ENHANCEMENT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_IPTV_ALFEC_ENHANCEMENT);
    pub const APPLICATION_VND_DVB_NOTIF_AGGREGATE_ROOT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_NOTIF_AGGREGATE_ROOT_XML);
    pub const APPLICATION_VND_DVB_NOTIF_CONTAINER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_NOTIF_CONTAINER_XML);
    pub const APPLICATION_VND_DVB_NOTIF_GENERIC_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_NOTIF_GENERIC_XML);
    pub const APPLICATION_VND_DVB_NOTIF_IA_MSGLIST_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_NOTIF_IA_MSGLIST_XML);
    pub const APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_REQUEST_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_REQUEST_XML);
    pub const APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_RESPONSE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_RESPONSE_XML);
    pub const APPLICATION_VND_DVB_NOTIF_INIT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_NOTIF_INIT_XML);
    pub const APPLICATION_VND_DVB_PFR: Encoding = Encoding::new(prefix::APPLICATION_VND_DVB_PFR);
    pub const APPLICATION_VND_DVB_SERVICE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_DVB_SERVICE);
    pub const APPLICATION_VND_DXR: Encoding = Encoding::new(prefix::APPLICATION_VND_DXR);
    pub const APPLICATION_VND_DYNAGEO: Encoding = Encoding::new(prefix::APPLICATION_VND_DYNAGEO);
    pub const APPLICATION_VND_DZR: Encoding = Encoding::new(prefix::APPLICATION_VND_DZR);
    pub const APPLICATION_VND_EASYKARAOKE_CDGDOWNLOAD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EASYKARAOKE_CDGDOWNLOAD);
    pub const APPLICATION_VND_ECDIS_UPDATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ECDIS_UPDATE);
    pub const APPLICATION_VND_ECIP_RLP: Encoding = Encoding::new(prefix::APPLICATION_VND_ECIP_RLP);
    pub const APPLICATION_VND_ECLIPSE_DITTO_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ECLIPSE_DITTO_JSON);
    pub const APPLICATION_VND_ECOWIN_CHART: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ECOWIN_CHART);
    pub const APPLICATION_VND_ECOWIN_FILEREQUEST: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ECOWIN_FILEREQUEST);
    pub const APPLICATION_VND_ECOWIN_FILEUPDATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ECOWIN_FILEUPDATE);
    pub const APPLICATION_VND_ECOWIN_SERIES: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ECOWIN_SERIES);
    pub const APPLICATION_VND_ECOWIN_SERIESREQUEST: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ECOWIN_SERIESREQUEST);
    pub const APPLICATION_VND_ECOWIN_SERIESUPDATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ECOWIN_SERIESUPDATE);
    pub const APPLICATION_VND_EFI_IMG: Encoding = Encoding::new(prefix::APPLICATION_VND_EFI_IMG);
    pub const APPLICATION_VND_EFI_ISO: Encoding = Encoding::new(prefix::APPLICATION_VND_EFI_ISO);
    pub const APPLICATION_VND_ELN_ZIP: Encoding = Encoding::new(prefix::APPLICATION_VND_ELN_ZIP);
    pub const APPLICATION_VND_EMCLIENT_ACCESSREQUEST_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EMCLIENT_ACCESSREQUEST_XML);
    pub const APPLICATION_VND_ENLIVEN: Encoding = Encoding::new(prefix::APPLICATION_VND_ENLIVEN);
    pub const APPLICATION_VND_ENPHASE_ENVOY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ENPHASE_ENVOY);
    pub const APPLICATION_VND_EPRINTS_DATA_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EPRINTS_DATA_XML);
    pub const APPLICATION_VND_EPSON_ESF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EPSON_ESF);
    pub const APPLICATION_VND_EPSON_MSF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EPSON_MSF);
    pub const APPLICATION_VND_EPSON_QUICKANIME: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EPSON_QUICKANIME);
    pub const APPLICATION_VND_EPSON_SALT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EPSON_SALT);
    pub const APPLICATION_VND_EPSON_SSF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EPSON_SSF);
    pub const APPLICATION_VND_ERICSSON_QUICKCALL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ERICSSON_QUICKCALL);
    pub const APPLICATION_VND_EROFS: Encoding = Encoding::new(prefix::APPLICATION_VND_EROFS);
    pub const APPLICATION_VND_ESPASS_ESPASS_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ESPASS_ESPASS_ZIP);
    pub const APPLICATION_VND_ESZIGNO3_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ESZIGNO3_XML);
    pub const APPLICATION_VND_ETSI_AOC_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_AOC_XML);
    pub const APPLICATION_VND_ETSI_ASIC_E_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_ASIC_E_ZIP);
    pub const APPLICATION_VND_ETSI_ASIC_S_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_ASIC_S_ZIP);
    pub const APPLICATION_VND_ETSI_CUG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_CUG_XML);
    pub const APPLICATION_VND_ETSI_IPTVCOMMAND_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_IPTVCOMMAND_XML);
    pub const APPLICATION_VND_ETSI_IPTVDISCOVERY_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_IPTVDISCOVERY_XML);
    pub const APPLICATION_VND_ETSI_IPTVPROFILE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_IPTVPROFILE_XML);
    pub const APPLICATION_VND_ETSI_IPTVSAD_BC_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_IPTVSAD_BC_XML);
    pub const APPLICATION_VND_ETSI_IPTVSAD_COD_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_IPTVSAD_COD_XML);
    pub const APPLICATION_VND_ETSI_IPTVSAD_NPVR_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_IPTVSAD_NPVR_XML);
    pub const APPLICATION_VND_ETSI_IPTVSERVICE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_IPTVSERVICE_XML);
    pub const APPLICATION_VND_ETSI_IPTVSYNC_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_IPTVSYNC_XML);
    pub const APPLICATION_VND_ETSI_IPTVUEPROFILE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_IPTVUEPROFILE_XML);
    pub const APPLICATION_VND_ETSI_MCID_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_MCID_XML);
    pub const APPLICATION_VND_ETSI_MHEG5: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_MHEG5);
    pub const APPLICATION_VND_ETSI_OVERLOAD_CONTROL_POLICY_DATASET_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_OVERLOAD_CONTROL_POLICY_DATASET_XML);
    pub const APPLICATION_VND_ETSI_PSTN_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_PSTN_XML);
    pub const APPLICATION_VND_ETSI_SCI_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_SCI_XML);
    pub const APPLICATION_VND_ETSI_SIMSERVS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_SIMSERVS_XML);
    pub const APPLICATION_VND_ETSI_TIMESTAMP_TOKEN: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_TIMESTAMP_TOKEN);
    pub const APPLICATION_VND_ETSI_TSL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_TSL_XML);
    pub const APPLICATION_VND_ETSI_TSL_DER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ETSI_TSL_DER);
    pub const APPLICATION_VND_EU_KASPARIAN_CAR_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EU_KASPARIAN_CAR_JSON);
    pub const APPLICATION_VND_EUDORA_DATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EUDORA_DATA);
    pub const APPLICATION_VND_EVOLV_ECIG_PROFILE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EVOLV_ECIG_PROFILE);
    pub const APPLICATION_VND_EVOLV_ECIG_SETTINGS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EVOLV_ECIG_SETTINGS);
    pub const APPLICATION_VND_EVOLV_ECIG_THEME: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EVOLV_ECIG_THEME);
    pub const APPLICATION_VND_EXSTREAM_EMPOWER_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EXSTREAM_EMPOWER_ZIP);
    pub const APPLICATION_VND_EXSTREAM_PACKAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EXSTREAM_PACKAGE);
    pub const APPLICATION_VND_EZPIX_ALBUM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EZPIX_ALBUM);
    pub const APPLICATION_VND_EZPIX_PACKAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_EZPIX_PACKAGE);
    pub const APPLICATION_VND_F_SECURE_MOBILE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_F_SECURE_MOBILE);
    pub const APPLICATION_VND_FAMILYSEARCH_GEDCOM_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FAMILYSEARCH_GEDCOM_ZIP);
    pub const APPLICATION_VND_FASTCOPY_DISK_IMAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FASTCOPY_DISK_IMAGE);
    pub const APPLICATION_VND_FDSN_MSEED: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FDSN_MSEED);
    pub const APPLICATION_VND_FDSN_SEED: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FDSN_SEED);
    pub const APPLICATION_VND_FFSNS: Encoding = Encoding::new(prefix::APPLICATION_VND_FFSNS);
    pub const APPLICATION_VND_FICLAB_FLB_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FICLAB_FLB_ZIP);
    pub const APPLICATION_VND_FILMIT_ZFC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FILMIT_ZFC);
    pub const APPLICATION_VND_FINTS: Encoding = Encoding::new(prefix::APPLICATION_VND_FINTS);
    pub const APPLICATION_VND_FIREMONKEYS_CLOUDCELL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FIREMONKEYS_CLOUDCELL);
    pub const APPLICATION_VND_FLUXTIME_CLIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FLUXTIME_CLIP);
    pub const APPLICATION_VND_FONT_FONTFORGE_SFD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FONT_FONTFORGE_SFD);
    pub const APPLICATION_VND_FRAMEMAKER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FRAMEMAKER);
    pub const APPLICATION_VND_FREELOG_COMIC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FREELOG_COMIC);
    pub const APPLICATION_VND_FROGANS_FNC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FROGANS_FNC);
    pub const APPLICATION_VND_FROGANS_LTF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FROGANS_LTF);
    pub const APPLICATION_VND_FSC_WEBLAUNCH: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FSC_WEBLAUNCH);
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJIFILM_FB_DOCUWORKS);
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_BINDER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_BINDER);
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_CONTAINER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_CONTAINER);
    pub const APPLICATION_VND_FUJIFILM_FB_JFI_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJIFILM_FB_JFI_XML);
    pub const APPLICATION_VND_FUJITSU_OASYS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJITSU_OASYS);
    pub const APPLICATION_VND_FUJITSU_OASYS2: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJITSU_OASYS2);
    pub const APPLICATION_VND_FUJITSU_OASYS3: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJITSU_OASYS3);
    pub const APPLICATION_VND_FUJITSU_OASYSGP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJITSU_OASYSGP);
    pub const APPLICATION_VND_FUJITSU_OASYSPRS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJITSU_OASYSPRS);
    pub const APPLICATION_VND_FUJIXEROX_ART_EX: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJIXEROX_ART_EX);
    pub const APPLICATION_VND_FUJIXEROX_ART4: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJIXEROX_ART4);
    pub const APPLICATION_VND_FUJIXEROX_HBPL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJIXEROX_HBPL);
    pub const APPLICATION_VND_FUJIXEROX_DDD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJIXEROX_DDD);
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJIXEROX_DOCUWORKS);
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS_BINDER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJIXEROX_DOCUWORKS_BINDER);
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS_CONTAINER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUJIXEROX_DOCUWORKS_CONTAINER);
    pub const APPLICATION_VND_FUT_MISNET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUT_MISNET);
    pub const APPLICATION_VND_FUTOIN_CBOR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUTOIN_CBOR);
    pub const APPLICATION_VND_FUTOIN_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUTOIN_JSON);
    pub const APPLICATION_VND_FUZZYSHEET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_FUZZYSHEET);
    pub const APPLICATION_VND_GENOMATIX_TUXEDO: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GENOMATIX_TUXEDO);
    pub const APPLICATION_VND_GENOZIP: Encoding = Encoding::new(prefix::APPLICATION_VND_GENOZIP);
    pub const APPLICATION_VND_GENTICS_GRD_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GENTICS_GRD_JSON);
    pub const APPLICATION_VND_GENTOO_CATMETADATA_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GENTOO_CATMETADATA_XML);
    pub const APPLICATION_VND_GENTOO_EBUILD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GENTOO_EBUILD);
    pub const APPLICATION_VND_GENTOO_ECLASS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GENTOO_ECLASS);
    pub const APPLICATION_VND_GENTOO_GPKG: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GENTOO_GPKG);
    pub const APPLICATION_VND_GENTOO_MANIFEST: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GENTOO_MANIFEST);
    pub const APPLICATION_VND_GENTOO_PKGMETADATA_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GENTOO_PKGMETADATA_XML);
    pub const APPLICATION_VND_GENTOO_XPAK: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GENTOO_XPAK);
    pub const APPLICATION_VND_GEO_JSON: Encoding = Encoding::new(prefix::APPLICATION_VND_GEO_JSON);
    pub const APPLICATION_VND_GEOCUBE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GEOCUBE_XML);
    pub const APPLICATION_VND_GEOGEBRA_FILE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GEOGEBRA_FILE);
    pub const APPLICATION_VND_GEOGEBRA_SLIDES: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GEOGEBRA_SLIDES);
    pub const APPLICATION_VND_GEOGEBRA_TOOL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GEOGEBRA_TOOL);
    pub const APPLICATION_VND_GEOMETRY_EXPLORER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GEOMETRY_EXPLORER);
    pub const APPLICATION_VND_GEONEXT: Encoding = Encoding::new(prefix::APPLICATION_VND_GEONEXT);
    pub const APPLICATION_VND_GEOPLAN: Encoding = Encoding::new(prefix::APPLICATION_VND_GEOPLAN);
    pub const APPLICATION_VND_GEOSPACE: Encoding = Encoding::new(prefix::APPLICATION_VND_GEOSPACE);
    pub const APPLICATION_VND_GERBER: Encoding = Encoding::new(prefix::APPLICATION_VND_GERBER);
    pub const APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT);
    pub const APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT_RESPONSE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT_RESPONSE);
    pub const APPLICATION_VND_GMX: Encoding = Encoding::new(prefix::APPLICATION_VND_GMX);
    pub const APPLICATION_VND_GNU_TALER_EXCHANGE_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GNU_TALER_EXCHANGE_JSON);
    pub const APPLICATION_VND_GNU_TALER_MERCHANT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GNU_TALER_MERCHANT_JSON);
    pub const APPLICATION_VND_GOOGLE_EARTH_KML_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GOOGLE_EARTH_KML_XML);
    pub const APPLICATION_VND_GOOGLE_EARTH_KMZ: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GOOGLE_EARTH_KMZ);
    pub const APPLICATION_VND_GOV_SK_E_FORM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GOV_SK_E_FORM_XML);
    pub const APPLICATION_VND_GOV_SK_E_FORM_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GOV_SK_E_FORM_ZIP);
    pub const APPLICATION_VND_GOV_SK_XMLDATACONTAINER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GOV_SK_XMLDATACONTAINER_XML);
    pub const APPLICATION_VND_GPXSEE_MAP_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GPXSEE_MAP_XML);
    pub const APPLICATION_VND_GRAFEQ: Encoding = Encoding::new(prefix::APPLICATION_VND_GRAFEQ);
    pub const APPLICATION_VND_GRIDMP: Encoding = Encoding::new(prefix::APPLICATION_VND_GRIDMP);
    pub const APPLICATION_VND_GROOVE_ACCOUNT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GROOVE_ACCOUNT);
    pub const APPLICATION_VND_GROOVE_HELP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GROOVE_HELP);
    pub const APPLICATION_VND_GROOVE_IDENTITY_MESSAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GROOVE_IDENTITY_MESSAGE);
    pub const APPLICATION_VND_GROOVE_INJECTOR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GROOVE_INJECTOR);
    pub const APPLICATION_VND_GROOVE_TOOL_MESSAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GROOVE_TOOL_MESSAGE);
    pub const APPLICATION_VND_GROOVE_TOOL_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GROOVE_TOOL_TEMPLATE);
    pub const APPLICATION_VND_GROOVE_VCARD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_GROOVE_VCARD);
    pub const APPLICATION_VND_HAL_JSON: Encoding = Encoding::new(prefix::APPLICATION_VND_HAL_JSON);
    pub const APPLICATION_VND_HAL_XML: Encoding = Encoding::new(prefix::APPLICATION_VND_HAL_XML);
    pub const APPLICATION_VND_HBCI: Encoding = Encoding::new(prefix::APPLICATION_VND_HBCI);
    pub const APPLICATION_VND_HC_JSON: Encoding = Encoding::new(prefix::APPLICATION_VND_HC_JSON);
    pub const APPLICATION_VND_HCL_BIREPORTS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_HCL_BIREPORTS);
    pub const APPLICATION_VND_HDT: Encoding = Encoding::new(prefix::APPLICATION_VND_HDT);
    pub const APPLICATION_VND_HEROKU_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_HEROKU_JSON);
    pub const APPLICATION_VND_HHE_LESSON_PLAYER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_HHE_LESSON_PLAYER);
    pub const APPLICATION_VND_HP_HPGL: Encoding = Encoding::new(prefix::APPLICATION_VND_HP_HPGL);
    pub const APPLICATION_VND_HP_PCL: Encoding = Encoding::new(prefix::APPLICATION_VND_HP_PCL);
    pub const APPLICATION_VND_HP_PCLXL: Encoding = Encoding::new(prefix::APPLICATION_VND_HP_PCLXL);
    pub const APPLICATION_VND_HP_HPID: Encoding = Encoding::new(prefix::APPLICATION_VND_HP_HPID);
    pub const APPLICATION_VND_HP_HPS: Encoding = Encoding::new(prefix::APPLICATION_VND_HP_HPS);
    pub const APPLICATION_VND_HP_JLYT: Encoding = Encoding::new(prefix::APPLICATION_VND_HP_JLYT);
    pub const APPLICATION_VND_HSL: Encoding = Encoding::new(prefix::APPLICATION_VND_HSL);
    pub const APPLICATION_VND_HTTPHONE: Encoding = Encoding::new(prefix::APPLICATION_VND_HTTPHONE);
    pub const APPLICATION_VND_HYDROSTATIX_SOF_DATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_HYDROSTATIX_SOF_DATA);
    pub const APPLICATION_VND_HYPER_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_HYPER_JSON);
    pub const APPLICATION_VND_HYPER_ITEM_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_HYPER_ITEM_JSON);
    pub const APPLICATION_VND_HYPERDRIVE_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_HYPERDRIVE_JSON);
    pub const APPLICATION_VND_HZN_3D_CROSSWORD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_HZN_3D_CROSSWORD);
    pub const APPLICATION_VND_IBM_MINIPAY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IBM_MINIPAY);
    pub const APPLICATION_VND_IBM_AFPLINEDATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IBM_AFPLINEDATA);
    pub const APPLICATION_VND_IBM_ELECTRONIC_MEDIA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IBM_ELECTRONIC_MEDIA);
    pub const APPLICATION_VND_IBM_MODCAP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IBM_MODCAP);
    pub const APPLICATION_VND_IBM_RIGHTS_MANAGEMENT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IBM_RIGHTS_MANAGEMENT);
    pub const APPLICATION_VND_IBM_SECURE_CONTAINER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IBM_SECURE_CONTAINER);
    pub const APPLICATION_VND_ICCPROFILE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ICCPROFILE);
    pub const APPLICATION_VND_IEEE_1905: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IEEE_1905);
    pub const APPLICATION_VND_IGLOADER: Encoding = Encoding::new(prefix::APPLICATION_VND_IGLOADER);
    pub const APPLICATION_VND_IMAGEMETER_FOLDER_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMAGEMETER_FOLDER_ZIP);
    pub const APPLICATION_VND_IMAGEMETER_IMAGE_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMAGEMETER_IMAGE_ZIP);
    pub const APPLICATION_VND_IMMERVISION_IVP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMMERVISION_IVP);
    pub const APPLICATION_VND_IMMERVISION_IVU: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMMERVISION_IVU);
    pub const APPLICATION_VND_IMS_IMSCCV1P1: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMS_IMSCCV1P1);
    pub const APPLICATION_VND_IMS_IMSCCV1P2: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMS_IMSCCV1P2);
    pub const APPLICATION_VND_IMS_IMSCCV1P3: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMS_IMSCCV1P3);
    pub const APPLICATION_VND_IMS_LIS_V2_RESULT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMS_LIS_V2_RESULT_JSON);
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLCONSUMERPROFILE_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMS_LTI_V2_TOOLCONSUMERPROFILE_JSON);
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_JSON);
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_ID_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_ID_JSON);
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_JSON);
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_SIMPLE_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_SIMPLE_JSON);
    pub const APPLICATION_VND_INFORMEDCONTROL_RMS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_INFORMEDCONTROL_RMS_XML);
    pub const APPLICATION_VND_INFORMIX_VISIONARY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_INFORMIX_VISIONARY);
    pub const APPLICATION_VND_INFOTECH_PROJECT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_INFOTECH_PROJECT);
    pub const APPLICATION_VND_INFOTECH_PROJECT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_INFOTECH_PROJECT_XML);
    pub const APPLICATION_VND_INNOPATH_WAMP_NOTIFICATION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_INNOPATH_WAMP_NOTIFICATION);
    pub const APPLICATION_VND_INSORS_IGM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_INSORS_IGM);
    pub const APPLICATION_VND_INTERCON_FORMNET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_INTERCON_FORMNET);
    pub const APPLICATION_VND_INTERGEO: Encoding = Encoding::new(prefix::APPLICATION_VND_INTERGEO);
    pub const APPLICATION_VND_INTERTRUST_DIGIBOX: Encoding =
        Encoding::new(prefix::APPLICATION_VND_INTERTRUST_DIGIBOX);
    pub const APPLICATION_VND_INTERTRUST_NNCP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_INTERTRUST_NNCP);
    pub const APPLICATION_VND_INTU_QBO: Encoding = Encoding::new(prefix::APPLICATION_VND_INTU_QBO);
    pub const APPLICATION_VND_INTU_QFX: Encoding = Encoding::new(prefix::APPLICATION_VND_INTU_QFX);
    pub const APPLICATION_VND_IPFS_IPNS_RECORD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IPFS_IPNS_RECORD);
    pub const APPLICATION_VND_IPLD_CAR: Encoding = Encoding::new(prefix::APPLICATION_VND_IPLD_CAR);
    pub const APPLICATION_VND_IPLD_DAG_CBOR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IPLD_DAG_CBOR);
    pub const APPLICATION_VND_IPLD_DAG_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IPLD_DAG_JSON);
    pub const APPLICATION_VND_IPLD_RAW: Encoding = Encoding::new(prefix::APPLICATION_VND_IPLD_RAW);
    pub const APPLICATION_VND_IPTC_G2_CATALOGITEM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IPTC_G2_CATALOGITEM_XML);
    pub const APPLICATION_VND_IPTC_G2_CONCEPTITEM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IPTC_G2_CONCEPTITEM_XML);
    pub const APPLICATION_VND_IPTC_G2_KNOWLEDGEITEM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IPTC_G2_KNOWLEDGEITEM_XML);
    pub const APPLICATION_VND_IPTC_G2_NEWSITEM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IPTC_G2_NEWSITEM_XML);
    pub const APPLICATION_VND_IPTC_G2_NEWSMESSAGE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IPTC_G2_NEWSMESSAGE_XML);
    pub const APPLICATION_VND_IPTC_G2_PACKAGEITEM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IPTC_G2_PACKAGEITEM_XML);
    pub const APPLICATION_VND_IPTC_G2_PLANNINGITEM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IPTC_G2_PLANNINGITEM_XML);
    pub const APPLICATION_VND_IPUNPLUGGED_RCPROFILE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IPUNPLUGGED_RCPROFILE);
    pub const APPLICATION_VND_IREPOSITORY_PACKAGE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_IREPOSITORY_PACKAGE_XML);
    pub const APPLICATION_VND_IS_XPR: Encoding = Encoding::new(prefix::APPLICATION_VND_IS_XPR);
    pub const APPLICATION_VND_ISAC_FCS: Encoding = Encoding::new(prefix::APPLICATION_VND_ISAC_FCS);
    pub const APPLICATION_VND_ISO11783_10_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ISO11783_10_ZIP);
    pub const APPLICATION_VND_JAM: Encoding = Encoding::new(prefix::APPLICATION_VND_JAM);
    pub const APPLICATION_VND_JAPANNET_DIRECTORY_SERVICE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_JAPANNET_DIRECTORY_SERVICE);
    pub const APPLICATION_VND_JAPANNET_JPNSTORE_WAKEUP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_JAPANNET_JPNSTORE_WAKEUP);
    pub const APPLICATION_VND_JAPANNET_PAYMENT_WAKEUP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_JAPANNET_PAYMENT_WAKEUP);
    pub const APPLICATION_VND_JAPANNET_REGISTRATION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_JAPANNET_REGISTRATION);
    pub const APPLICATION_VND_JAPANNET_REGISTRATION_WAKEUP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_JAPANNET_REGISTRATION_WAKEUP);
    pub const APPLICATION_VND_JAPANNET_SETSTORE_WAKEUP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_JAPANNET_SETSTORE_WAKEUP);
    pub const APPLICATION_VND_JAPANNET_VERIFICATION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_JAPANNET_VERIFICATION);
    pub const APPLICATION_VND_JAPANNET_VERIFICATION_WAKEUP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_JAPANNET_VERIFICATION_WAKEUP);
    pub const APPLICATION_VND_JCP_JAVAME_MIDLET_RMS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_JCP_JAVAME_MIDLET_RMS);
    pub const APPLICATION_VND_JISP: Encoding = Encoding::new(prefix::APPLICATION_VND_JISP);
    pub const APPLICATION_VND_JOOST_JODA_ARCHIVE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_JOOST_JODA_ARCHIVE);
    pub const APPLICATION_VND_JSK_ISDN_NGN: Encoding =
        Encoding::new(prefix::APPLICATION_VND_JSK_ISDN_NGN);
    pub const APPLICATION_VND_KAHOOTZ: Encoding = Encoding::new(prefix::APPLICATION_VND_KAHOOTZ);
    pub const APPLICATION_VND_KDE_KARBON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_KDE_KARBON);
    pub const APPLICATION_VND_KDE_KCHART: Encoding =
        Encoding::new(prefix::APPLICATION_VND_KDE_KCHART);
    pub const APPLICATION_VND_KDE_KFORMULA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_KDE_KFORMULA);
    pub const APPLICATION_VND_KDE_KIVIO: Encoding =
        Encoding::new(prefix::APPLICATION_VND_KDE_KIVIO);
    pub const APPLICATION_VND_KDE_KONTOUR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_KDE_KONTOUR);
    pub const APPLICATION_VND_KDE_KPRESENTER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_KDE_KPRESENTER);
    pub const APPLICATION_VND_KDE_KSPREAD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_KDE_KSPREAD);
    pub const APPLICATION_VND_KDE_KWORD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_KDE_KWORD);
    pub const APPLICATION_VND_KENAMEAAPP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_KENAMEAAPP);
    pub const APPLICATION_VND_KIDSPIRATION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_KIDSPIRATION);
    pub const APPLICATION_VND_KOAN: Encoding = Encoding::new(prefix::APPLICATION_VND_KOAN);
    pub const APPLICATION_VND_KODAK_DESCRIPTOR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_KODAK_DESCRIPTOR);
    pub const APPLICATION_VND_LAS: Encoding = Encoding::new(prefix::APPLICATION_VND_LAS);
    pub const APPLICATION_VND_LAS_LAS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LAS_LAS_JSON);
    pub const APPLICATION_VND_LAS_LAS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LAS_LAS_XML);
    pub const APPLICATION_VND_LASZIP: Encoding = Encoding::new(prefix::APPLICATION_VND_LASZIP);
    pub const APPLICATION_VND_LDEV_PRODUCTLICENSING: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LDEV_PRODUCTLICENSING);
    pub const APPLICATION_VND_LEAP_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LEAP_JSON);
    pub const APPLICATION_VND_LIBERTY_REQUEST_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LIBERTY_REQUEST_XML);
    pub const APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_DESKTOP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_DESKTOP);
    pub const APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_EXCHANGE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_EXCHANGE_XML);
    pub const APPLICATION_VND_LOGIPIPE_CIRCUIT_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LOGIPIPE_CIRCUIT_ZIP);
    pub const APPLICATION_VND_LOOM: Encoding = Encoding::new(prefix::APPLICATION_VND_LOOM);
    pub const APPLICATION_VND_LOTUS_1_2_3: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LOTUS_1_2_3);
    pub const APPLICATION_VND_LOTUS_APPROACH: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LOTUS_APPROACH);
    pub const APPLICATION_VND_LOTUS_FREELANCE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LOTUS_FREELANCE);
    pub const APPLICATION_VND_LOTUS_NOTES: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LOTUS_NOTES);
    pub const APPLICATION_VND_LOTUS_ORGANIZER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LOTUS_ORGANIZER);
    pub const APPLICATION_VND_LOTUS_SCREENCAM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LOTUS_SCREENCAM);
    pub const APPLICATION_VND_LOTUS_WORDPRO: Encoding =
        Encoding::new(prefix::APPLICATION_VND_LOTUS_WORDPRO);
    pub const APPLICATION_VND_MACPORTS_PORTPKG: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MACPORTS_PORTPKG);
    pub const APPLICATION_VND_MAPBOX_VECTOR_TILE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MAPBOX_VECTOR_TILE);
    pub const APPLICATION_VND_MARLIN_DRM_ACTIONTOKEN_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MARLIN_DRM_ACTIONTOKEN_XML);
    pub const APPLICATION_VND_MARLIN_DRM_CONFTOKEN_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MARLIN_DRM_CONFTOKEN_XML);
    pub const APPLICATION_VND_MARLIN_DRM_LICENSE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MARLIN_DRM_LICENSE_XML);
    pub const APPLICATION_VND_MARLIN_DRM_MDCF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MARLIN_DRM_MDCF);
    pub const APPLICATION_VND_MASON_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MASON_JSON);
    pub const APPLICATION_VND_MAXAR_ARCHIVE_3TZ_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MAXAR_ARCHIVE_3TZ_ZIP);
    pub const APPLICATION_VND_MAXMIND_MAXMIND_DB: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MAXMIND_MAXMIND_DB);
    pub const APPLICATION_VND_MCD: Encoding = Encoding::new(prefix::APPLICATION_VND_MCD);
    pub const APPLICATION_VND_MDL: Encoding = Encoding::new(prefix::APPLICATION_VND_MDL);
    pub const APPLICATION_VND_MDL_MBSDF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MDL_MBSDF);
    pub const APPLICATION_VND_MEDCALCDATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MEDCALCDATA);
    pub const APPLICATION_VND_MEDIASTATION_CDKEY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MEDIASTATION_CDKEY);
    pub const APPLICATION_VND_MEDICALHOLODECK_RECORDXR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MEDICALHOLODECK_RECORDXR);
    pub const APPLICATION_VND_MERIDIAN_SLINGSHOT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MERIDIAN_SLINGSHOT);
    pub const APPLICATION_VND_MERMAID: Encoding = Encoding::new(prefix::APPLICATION_VND_MERMAID);
    pub const APPLICATION_VND_MFMP: Encoding = Encoding::new(prefix::APPLICATION_VND_MFMP);
    pub const APPLICATION_VND_MICRO_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MICRO_JSON);
    pub const APPLICATION_VND_MICROGRAFX_FLO: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MICROGRAFX_FLO);
    pub const APPLICATION_VND_MICROGRAFX_IGX: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MICROGRAFX_IGX);
    pub const APPLICATION_VND_MICROSOFT_PORTABLE_EXECUTABLE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MICROSOFT_PORTABLE_EXECUTABLE);
    pub const APPLICATION_VND_MICROSOFT_WINDOWS_THUMBNAIL_CACHE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MICROSOFT_WINDOWS_THUMBNAIL_CACHE);
    pub const APPLICATION_VND_MIELE_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MIELE_JSON);
    pub const APPLICATION_VND_MIF: Encoding = Encoding::new(prefix::APPLICATION_VND_MIF);
    pub const APPLICATION_VND_MINISOFT_HP3000_SAVE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MINISOFT_HP3000_SAVE);
    pub const APPLICATION_VND_MITSUBISHI_MISTY_GUARD_TRUSTWEB: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MITSUBISHI_MISTY_GUARD_TRUSTWEB);
    pub const APPLICATION_VND_MODL: Encoding = Encoding::new(prefix::APPLICATION_VND_MODL);
    pub const APPLICATION_VND_MOPHUN_APPLICATION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOPHUN_APPLICATION);
    pub const APPLICATION_VND_MOPHUN_CERTIFICATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOPHUN_CERTIFICATE);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOTOROLA_FLEXSUITE);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_ADSI: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOTOROLA_FLEXSUITE_ADSI);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_FIS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOTOROLA_FLEXSUITE_FIS);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_GOTAP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOTOROLA_FLEXSUITE_GOTAP);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_KMR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOTOROLA_FLEXSUITE_KMR);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_TTC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOTOROLA_FLEXSUITE_TTC);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_WEM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOTOROLA_FLEXSUITE_WEM);
    pub const APPLICATION_VND_MOTOROLA_IPRM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOTOROLA_IPRM);
    pub const APPLICATION_VND_MOZILLA_XUL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MOZILLA_XUL_XML);
    pub const APPLICATION_VND_MS_3MFDOCUMENT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_3MFDOCUMENT);
    pub const APPLICATION_VND_MS_PRINTDEVICECAPABILITIES_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_PRINTDEVICECAPABILITIES_XML);
    pub const APPLICATION_VND_MS_PRINTSCHEMATICKET_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_PRINTSCHEMATICKET_XML);
    pub const APPLICATION_VND_MS_ARTGALRY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_ARTGALRY);
    pub const APPLICATION_VND_MS_ASF: Encoding = Encoding::new(prefix::APPLICATION_VND_MS_ASF);
    pub const APPLICATION_VND_MS_CAB_COMPRESSED: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_CAB_COMPRESSED);
    pub const APPLICATION_VND_MS_EXCEL: Encoding = Encoding::new(prefix::APPLICATION_VND_MS_EXCEL);
    pub const APPLICATION_VND_MS_EXCEL_ADDIN_MACROENABLED_12: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_EXCEL_ADDIN_MACROENABLED_12);
    pub const APPLICATION_VND_MS_EXCEL_SHEET_BINARY_MACROENABLED_12: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_EXCEL_SHEET_BINARY_MACROENABLED_12);
    pub const APPLICATION_VND_MS_EXCEL_SHEET_MACROENABLED_12: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_EXCEL_SHEET_MACROENABLED_12);
    pub const APPLICATION_VND_MS_EXCEL_TEMPLATE_MACROENABLED_12: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_EXCEL_TEMPLATE_MACROENABLED_12);
    pub const APPLICATION_VND_MS_FONTOBJECT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_FONTOBJECT);
    pub const APPLICATION_VND_MS_HTMLHELP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_HTMLHELP);
    pub const APPLICATION_VND_MS_IMS: Encoding = Encoding::new(prefix::APPLICATION_VND_MS_IMS);
    pub const APPLICATION_VND_MS_LRM: Encoding = Encoding::new(prefix::APPLICATION_VND_MS_LRM);
    pub const APPLICATION_VND_MS_OFFICE_ACTIVEX_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_OFFICE_ACTIVEX_XML);
    pub const APPLICATION_VND_MS_OFFICETHEME: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_OFFICETHEME);
    pub const APPLICATION_VND_MS_PLAYREADY_INITIATOR_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_PLAYREADY_INITIATOR_XML);
    pub const APPLICATION_VND_MS_POWERPOINT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_POWERPOINT);
    pub const APPLICATION_VND_MS_POWERPOINT_ADDIN_MACROENABLED_12: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_POWERPOINT_ADDIN_MACROENABLED_12);
    pub const APPLICATION_VND_MS_POWERPOINT_PRESENTATION_MACROENABLED_12: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_POWERPOINT_PRESENTATION_MACROENABLED_12);
    pub const APPLICATION_VND_MS_POWERPOINT_SLIDE_MACROENABLED_12: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_POWERPOINT_SLIDE_MACROENABLED_12);
    pub const APPLICATION_VND_MS_POWERPOINT_SLIDESHOW_MACROENABLED_12: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_POWERPOINT_SLIDESHOW_MACROENABLED_12);
    pub const APPLICATION_VND_MS_POWERPOINT_TEMPLATE_MACROENABLED_12: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_POWERPOINT_TEMPLATE_MACROENABLED_12);
    pub const APPLICATION_VND_MS_PROJECT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_PROJECT);
    pub const APPLICATION_VND_MS_TNEF: Encoding = Encoding::new(prefix::APPLICATION_VND_MS_TNEF);
    pub const APPLICATION_VND_MS_WINDOWS_DEVICEPAIRING: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_WINDOWS_DEVICEPAIRING);
    pub const APPLICATION_VND_MS_WINDOWS_NWPRINTING_OOB: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_WINDOWS_NWPRINTING_OOB);
    pub const APPLICATION_VND_MS_WINDOWS_PRINTERPAIRING: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_WINDOWS_PRINTERPAIRING);
    pub const APPLICATION_VND_MS_WINDOWS_WSD_OOB: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_WINDOWS_WSD_OOB);
    pub const APPLICATION_VND_MS_WMDRM_LIC_CHLG_REQ: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_WMDRM_LIC_CHLG_REQ);
    pub const APPLICATION_VND_MS_WMDRM_LIC_RESP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_WMDRM_LIC_RESP);
    pub const APPLICATION_VND_MS_WMDRM_METER_CHLG_REQ: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_WMDRM_METER_CHLG_REQ);
    pub const APPLICATION_VND_MS_WMDRM_METER_RESP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_WMDRM_METER_RESP);
    pub const APPLICATION_VND_MS_WORD_DOCUMENT_MACROENABLED_12: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_WORD_DOCUMENT_MACROENABLED_12);
    pub const APPLICATION_VND_MS_WORD_TEMPLATE_MACROENABLED_12: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_WORD_TEMPLATE_MACROENABLED_12);
    pub const APPLICATION_VND_MS_WORKS: Encoding = Encoding::new(prefix::APPLICATION_VND_MS_WORKS);
    pub const APPLICATION_VND_MS_WPL: Encoding = Encoding::new(prefix::APPLICATION_VND_MS_WPL);
    pub const APPLICATION_VND_MS_XPSDOCUMENT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MS_XPSDOCUMENT);
    pub const APPLICATION_VND_MSA_DISK_IMAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MSA_DISK_IMAGE);
    pub const APPLICATION_VND_MSEQ: Encoding = Encoding::new(prefix::APPLICATION_VND_MSEQ);
    pub const APPLICATION_VND_MSIGN: Encoding = Encoding::new(prefix::APPLICATION_VND_MSIGN);
    pub const APPLICATION_VND_MULTIAD_CREATOR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MULTIAD_CREATOR);
    pub const APPLICATION_VND_MULTIAD_CREATOR_CIF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MULTIAD_CREATOR_CIF);
    pub const APPLICATION_VND_MUSIC_NIFF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MUSIC_NIFF);
    pub const APPLICATION_VND_MUSICIAN: Encoding = Encoding::new(prefix::APPLICATION_VND_MUSICIAN);
    pub const APPLICATION_VND_MUVEE_STYLE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_MUVEE_STYLE);
    pub const APPLICATION_VND_MYNFC: Encoding = Encoding::new(prefix::APPLICATION_VND_MYNFC);
    pub const APPLICATION_VND_NACAMAR_YBRID_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NACAMAR_YBRID_JSON);
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_CBOR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NATO_BINDINGDATAOBJECT_CBOR);
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NATO_BINDINGDATAOBJECT_JSON);
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NATO_BINDINGDATAOBJECT_XML);
    pub const APPLICATION_VND_NATO_OPENXMLFORMATS_PACKAGE_IEPD_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NATO_OPENXMLFORMATS_PACKAGE_IEPD_ZIP);
    pub const APPLICATION_VND_NCD_CONTROL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NCD_CONTROL);
    pub const APPLICATION_VND_NCD_REFERENCE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NCD_REFERENCE);
    pub const APPLICATION_VND_NEARST_INV_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NEARST_INV_JSON);
    pub const APPLICATION_VND_NEBUMIND_LINE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NEBUMIND_LINE);
    pub const APPLICATION_VND_NERVANA: Encoding = Encoding::new(prefix::APPLICATION_VND_NERVANA);
    pub const APPLICATION_VND_NETFPX: Encoding = Encoding::new(prefix::APPLICATION_VND_NETFPX);
    pub const APPLICATION_VND_NEUROLANGUAGE_NLU: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NEUROLANGUAGE_NLU);
    pub const APPLICATION_VND_NIMN: Encoding = Encoding::new(prefix::APPLICATION_VND_NIMN);
    pub const APPLICATION_VND_NINTENDO_NITRO_ROM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NINTENDO_NITRO_ROM);
    pub const APPLICATION_VND_NINTENDO_SNES_ROM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NINTENDO_SNES_ROM);
    pub const APPLICATION_VND_NITF: Encoding = Encoding::new(prefix::APPLICATION_VND_NITF);
    pub const APPLICATION_VND_NOBLENET_DIRECTORY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOBLENET_DIRECTORY);
    pub const APPLICATION_VND_NOBLENET_SEALER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOBLENET_SEALER);
    pub const APPLICATION_VND_NOBLENET_WEB: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOBLENET_WEB);
    pub const APPLICATION_VND_NOKIA_CATALOGS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_CATALOGS);
    pub const APPLICATION_VND_NOKIA_CONML_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_CONML_WBXML);
    pub const APPLICATION_VND_NOKIA_CONML_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_CONML_XML);
    pub const APPLICATION_VND_NOKIA_ISDS_RADIO_PRESETS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_ISDS_RADIO_PRESETS);
    pub const APPLICATION_VND_NOKIA_IPTV_CONFIG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_IPTV_CONFIG_XML);
    pub const APPLICATION_VND_NOKIA_LANDMARK_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_LANDMARK_WBXML);
    pub const APPLICATION_VND_NOKIA_LANDMARK_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_LANDMARK_XML);
    pub const APPLICATION_VND_NOKIA_LANDMARKCOLLECTION_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_LANDMARKCOLLECTION_XML);
    pub const APPLICATION_VND_NOKIA_N_GAGE_AC_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_N_GAGE_AC_XML);
    pub const APPLICATION_VND_NOKIA_N_GAGE_DATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_N_GAGE_DATA);
    pub const APPLICATION_VND_NOKIA_N_GAGE_SYMBIAN_INSTALL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_N_GAGE_SYMBIAN_INSTALL);
    pub const APPLICATION_VND_NOKIA_NCD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_NCD);
    pub const APPLICATION_VND_NOKIA_PCD_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_PCD_WBXML);
    pub const APPLICATION_VND_NOKIA_PCD_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_PCD_XML);
    pub const APPLICATION_VND_NOKIA_RADIO_PRESET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_RADIO_PRESET);
    pub const APPLICATION_VND_NOKIA_RADIO_PRESETS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOKIA_RADIO_PRESETS);
    pub const APPLICATION_VND_NOVADIGM_EDM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOVADIGM_EDM);
    pub const APPLICATION_VND_NOVADIGM_EDX: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOVADIGM_EDX);
    pub const APPLICATION_VND_NOVADIGM_EXT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NOVADIGM_EXT);
    pub const APPLICATION_VND_NTT_LOCAL_CONTENT_SHARE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NTT_LOCAL_CONTENT_SHARE);
    pub const APPLICATION_VND_NTT_LOCAL_FILE_TRANSFER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NTT_LOCAL_FILE_TRANSFER);
    pub const APPLICATION_VND_NTT_LOCAL_OGW_REMOTE_ACCESS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NTT_LOCAL_OGW_REMOTE_ACCESS);
    pub const APPLICATION_VND_NTT_LOCAL_SIP_TA_REMOTE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NTT_LOCAL_SIP_TA_REMOTE);
    pub const APPLICATION_VND_NTT_LOCAL_SIP_TA_TCP_STREAM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_NTT_LOCAL_SIP_TA_TCP_STREAM);
    pub const APPLICATION_VND_OAI_WORKFLOWS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OAI_WORKFLOWS);
    pub const APPLICATION_VND_OAI_WORKFLOWS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OAI_WORKFLOWS_JSON);
    pub const APPLICATION_VND_OAI_WORKFLOWS_YAML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OAI_WORKFLOWS_YAML);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_BASE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_BASE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_CHART: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_CHART);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_CHART_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_CHART_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_DATABASE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_DATABASE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_WEB: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_WEB);
    pub const APPLICATION_VND_OBN: Encoding = Encoding::new(prefix::APPLICATION_VND_OBN);
    pub const APPLICATION_VND_OCF_CBOR: Encoding = Encoding::new(prefix::APPLICATION_VND_OCF_CBOR);
    pub const APPLICATION_VND_OCI_IMAGE_MANIFEST_V1_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OCI_IMAGE_MANIFEST_V1_JSON);
    pub const APPLICATION_VND_OFTN_L10N_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OFTN_L10N_JSON);
    pub const APPLICATION_VND_OIPF_CONTENTACCESSDOWNLOAD_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OIPF_CONTENTACCESSDOWNLOAD_XML);
    pub const APPLICATION_VND_OIPF_CONTENTACCESSSTREAMING_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OIPF_CONTENTACCESSSTREAMING_XML);
    pub const APPLICATION_VND_OIPF_CSPG_HEXBINARY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OIPF_CSPG_HEXBINARY);
    pub const APPLICATION_VND_OIPF_DAE_SVG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OIPF_DAE_SVG_XML);
    pub const APPLICATION_VND_OIPF_DAE_XHTML_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OIPF_DAE_XHTML_XML);
    pub const APPLICATION_VND_OIPF_MIPPVCONTROLMESSAGE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OIPF_MIPPVCONTROLMESSAGE_XML);
    pub const APPLICATION_VND_OIPF_PAE_GEM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OIPF_PAE_GEM);
    pub const APPLICATION_VND_OIPF_SPDISCOVERY_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OIPF_SPDISCOVERY_XML);
    pub const APPLICATION_VND_OIPF_SPDLIST_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OIPF_SPDLIST_XML);
    pub const APPLICATION_VND_OIPF_UEPROFILE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OIPF_UEPROFILE_XML);
    pub const APPLICATION_VND_OIPF_USERPROFILE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OIPF_USERPROFILE_XML);
    pub const APPLICATION_VND_OLPC_SUGAR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OLPC_SUGAR);
    pub const APPLICATION_VND_OMA_SCWS_CONFIG: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_SCWS_CONFIG);
    pub const APPLICATION_VND_OMA_SCWS_HTTP_REQUEST: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_SCWS_HTTP_REQUEST);
    pub const APPLICATION_VND_OMA_SCWS_HTTP_RESPONSE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_SCWS_HTTP_RESPONSE);
    pub const APPLICATION_VND_OMA_BCAST_ASSOCIATED_PROCEDURE_PARAMETER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_ASSOCIATED_PROCEDURE_PARAMETER_XML);
    pub const APPLICATION_VND_OMA_BCAST_DRM_TRIGGER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_DRM_TRIGGER_XML);
    pub const APPLICATION_VND_OMA_BCAST_IMD_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_IMD_XML);
    pub const APPLICATION_VND_OMA_BCAST_LTKM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_LTKM);
    pub const APPLICATION_VND_OMA_BCAST_NOTIFICATION_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_NOTIFICATION_XML);
    pub const APPLICATION_VND_OMA_BCAST_PROVISIONINGTRIGGER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_PROVISIONINGTRIGGER);
    pub const APPLICATION_VND_OMA_BCAST_SGBOOT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_SGBOOT);
    pub const APPLICATION_VND_OMA_BCAST_SGDD_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_SGDD_XML);
    pub const APPLICATION_VND_OMA_BCAST_SGDU: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_SGDU);
    pub const APPLICATION_VND_OMA_BCAST_SIMPLE_SYMBOL_CONTAINER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_SIMPLE_SYMBOL_CONTAINER);
    pub const APPLICATION_VND_OMA_BCAST_SMARTCARD_TRIGGER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_SMARTCARD_TRIGGER_XML);
    pub const APPLICATION_VND_OMA_BCAST_SPROV_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_SPROV_XML);
    pub const APPLICATION_VND_OMA_BCAST_STKM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_BCAST_STKM);
    pub const APPLICATION_VND_OMA_CAB_ADDRESS_BOOK_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_CAB_ADDRESS_BOOK_XML);
    pub const APPLICATION_VND_OMA_CAB_FEATURE_HANDLER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_CAB_FEATURE_HANDLER_XML);
    pub const APPLICATION_VND_OMA_CAB_PCC_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_CAB_PCC_XML);
    pub const APPLICATION_VND_OMA_CAB_SUBS_INVITE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_CAB_SUBS_INVITE_XML);
    pub const APPLICATION_VND_OMA_CAB_USER_PREFS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_CAB_USER_PREFS_XML);
    pub const APPLICATION_VND_OMA_DCD: Encoding = Encoding::new(prefix::APPLICATION_VND_OMA_DCD);
    pub const APPLICATION_VND_OMA_DCDC: Encoding = Encoding::new(prefix::APPLICATION_VND_OMA_DCDC);
    pub const APPLICATION_VND_OMA_DD2_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_DD2_XML);
    pub const APPLICATION_VND_OMA_DRM_RISD_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_DRM_RISD_XML);
    pub const APPLICATION_VND_OMA_GROUP_USAGE_LIST_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_GROUP_USAGE_LIST_XML);
    pub const APPLICATION_VND_OMA_LWM2M_CBOR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_LWM2M_CBOR);
    pub const APPLICATION_VND_OMA_LWM2M_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_LWM2M_JSON);
    pub const APPLICATION_VND_OMA_LWM2M_TLV: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_LWM2M_TLV);
    pub const APPLICATION_VND_OMA_PAL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_PAL_XML);
    pub const APPLICATION_VND_OMA_POC_DETAILED_PROGRESS_REPORT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_POC_DETAILED_PROGRESS_REPORT_XML);
    pub const APPLICATION_VND_OMA_POC_FINAL_REPORT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_POC_FINAL_REPORT_XML);
    pub const APPLICATION_VND_OMA_POC_GROUPS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_POC_GROUPS_XML);
    pub const APPLICATION_VND_OMA_POC_INVOCATION_DESCRIPTOR_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_POC_INVOCATION_DESCRIPTOR_XML);
    pub const APPLICATION_VND_OMA_POC_OPTIMIZED_PROGRESS_REPORT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_POC_OPTIMIZED_PROGRESS_REPORT_XML);
    pub const APPLICATION_VND_OMA_PUSH: Encoding = Encoding::new(prefix::APPLICATION_VND_OMA_PUSH);
    pub const APPLICATION_VND_OMA_SCIDM_MESSAGES_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_SCIDM_MESSAGES_XML);
    pub const APPLICATION_VND_OMA_XCAP_DIRECTORY_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMA_XCAP_DIRECTORY_XML);
    pub const APPLICATION_VND_OMADS_EMAIL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMADS_EMAIL_XML);
    pub const APPLICATION_VND_OMADS_FILE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMADS_FILE_XML);
    pub const APPLICATION_VND_OMADS_FOLDER_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMADS_FOLDER_XML);
    pub const APPLICATION_VND_OMALOC_SUPL_INIT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OMALOC_SUPL_INIT);
    pub const APPLICATION_VND_ONEPAGER: Encoding = Encoding::new(prefix::APPLICATION_VND_ONEPAGER);
    pub const APPLICATION_VND_ONEPAGERTAMP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ONEPAGERTAMP);
    pub const APPLICATION_VND_ONEPAGERTAMX: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ONEPAGERTAMX);
    pub const APPLICATION_VND_ONEPAGERTAT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ONEPAGERTAT);
    pub const APPLICATION_VND_ONEPAGERTATP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ONEPAGERTATP);
    pub const APPLICATION_VND_ONEPAGERTATX: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ONEPAGERTATX);
    pub const APPLICATION_VND_ONVIF_METADATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ONVIF_METADATA);
    pub const APPLICATION_VND_OPENBLOX_GAME_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENBLOX_GAME_XML);
    pub const APPLICATION_VND_OPENBLOX_GAME_BINARY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENBLOX_GAME_BINARY);
    pub const APPLICATION_VND_OPENEYE_OEB: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENEYE_OEB);
    pub const APPLICATION_VND_OPENSTREETMAP_DATA_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENSTREETMAP_DATA_XML);
    pub const APPLICATION_VND_OPENTIMESTAMPS_OTS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENTIMESTAMPS_OTS);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOM_PROPERTIES_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOM_PROPERTIES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOMXMLPROPERTIES_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOMXMLPROPERTIES_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWING_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWING_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHART_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHART_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHARTSHAPES_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHARTSHAPES_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMCOLORS_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMCOLORS_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMDATA_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMDATA_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMLAYOUT_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMLAYOUT_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMSTYLE_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMSTYLE_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_EXTENDED_PROPERTIES_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_EXTENDED_PROPERTIES_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTAUTHORS_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTAUTHORS_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTS_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTS_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_HANDOUTMASTER_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_HANDOUTMASTER_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESMASTER_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESMASTER_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESSLIDE_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESSLIDE_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESPROPS_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESPROPS_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION_MAIN_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION_MAIN_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDELAYOUT_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDELAYOUT_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEMASTER_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEMASTER_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEUPDATEINFO_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEUPDATEINFO_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW_MAIN_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW_MAIN_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TABLESTYLES_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TABLESTYLES_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TAGS_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TAGS_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE_MAIN_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE_MAIN_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_VIEWPROPS_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_VIEWPROPS_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CALCCHAIN_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CALCCHAIN_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CHARTSHEET_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CHARTSHEET_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_COMMENTS_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_COMMENTS_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CONNECTIONS_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CONNECTIONS_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_DIALOGSHEET_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_DIALOGSHEET_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_EXTERNALLINK_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_EXTERNALLINK_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHEDEFINITION_XML: Encoding = Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHEDEFINITION_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHERECORDS_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHERECORDS_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTTABLE_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTTABLE_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_QUERYTABLE_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_QUERYTABLE_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONHEADERS_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONHEADERS_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONLOG_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONLOG_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHAREDSTRINGS_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHAREDSTRINGS_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET_MAIN_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET_MAIN_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEETMETADATA_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEETMETADATA_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_STYLES_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_STYLES_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLE_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLE_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLESINGLECELLS_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLESINGLECELLS_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE_MAIN_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE_MAIN_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_USERNAMES_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_USERNAMES_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_VOLATILEDEPENDENCIES_XML: Encoding = Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_VOLATILEDEPENDENCIES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_WORKSHEET_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_WORKSHEET_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEME_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEME_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEMEOVERRIDE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEMEOVERRIDE_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_VMLDRAWING: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_VMLDRAWING);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_COMMENTS_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_COMMENTS_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_GLOSSARY_XML: Encoding = Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_GLOSSARY_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_MAIN_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_MAIN_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_ENDNOTES_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_ENDNOTES_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FONTTABLE_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FONTTABLE_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTER_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTER_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTNOTES_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTNOTES_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_NUMBERING_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_NUMBERING_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_SETTINGS_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_SETTINGS_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_STYLES_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_STYLES_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE_MAIN_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE_MAIN_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_WEBSETTINGS_XML:
        Encoding = Encoding::new(
        prefix::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_WEBSETTINGS_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_CORE_PROPERTIES_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_PACKAGE_CORE_PROPERTIES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_DIGITAL_SIGNATURE_XMLSIGNATURE_XML: Encoding =
        Encoding::new(
            prefix::APPLICATION_VND_OPENXMLFORMATS_PACKAGE_DIGITAL_SIGNATURE_XMLSIGNATURE_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_RELATIONSHIPS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OPENXMLFORMATS_PACKAGE_RELATIONSHIPS_XML);
    pub const APPLICATION_VND_ORACLE_RESOURCE_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ORACLE_RESOURCE_JSON);
    pub const APPLICATION_VND_ORANGE_INDATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ORANGE_INDATA);
    pub const APPLICATION_VND_OSA_NETDEPLOY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OSA_NETDEPLOY);
    pub const APPLICATION_VND_OSGEO_MAPGUIDE_PACKAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OSGEO_MAPGUIDE_PACKAGE);
    pub const APPLICATION_VND_OSGI_BUNDLE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OSGI_BUNDLE);
    pub const APPLICATION_VND_OSGI_DP: Encoding = Encoding::new(prefix::APPLICATION_VND_OSGI_DP);
    pub const APPLICATION_VND_OSGI_SUBSYSTEM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OSGI_SUBSYSTEM);
    pub const APPLICATION_VND_OTPS_CT_KIP_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OTPS_CT_KIP_XML);
    pub const APPLICATION_VND_OXLI_COUNTGRAPH: Encoding =
        Encoding::new(prefix::APPLICATION_VND_OXLI_COUNTGRAPH);
    pub const APPLICATION_VND_PAGERDUTY_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PAGERDUTY_JSON);
    pub const APPLICATION_VND_PALM: Encoding = Encoding::new(prefix::APPLICATION_VND_PALM);
    pub const APPLICATION_VND_PANOPLY: Encoding = Encoding::new(prefix::APPLICATION_VND_PANOPLY);
    pub const APPLICATION_VND_PAOS_XML: Encoding = Encoding::new(prefix::APPLICATION_VND_PAOS_XML);
    pub const APPLICATION_VND_PATENTDIVE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PATENTDIVE);
    pub const APPLICATION_VND_PATIENTECOMMSDOC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PATIENTECOMMSDOC);
    pub const APPLICATION_VND_PAWAAFILE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PAWAAFILE);
    pub const APPLICATION_VND_PCOS: Encoding = Encoding::new(prefix::APPLICATION_VND_PCOS);
    pub const APPLICATION_VND_PG_FORMAT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PG_FORMAT);
    pub const APPLICATION_VND_PG_OSASLI: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PG_OSASLI);
    pub const APPLICATION_VND_PIACCESS_APPLICATION_LICENCE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PIACCESS_APPLICATION_LICENCE);
    pub const APPLICATION_VND_PICSEL: Encoding = Encoding::new(prefix::APPLICATION_VND_PICSEL);
    pub const APPLICATION_VND_PMI_WIDGET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PMI_WIDGET);
    pub const APPLICATION_VND_POC_GROUP_ADVERTISEMENT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_POC_GROUP_ADVERTISEMENT_XML);
    pub const APPLICATION_VND_POCKETLEARN: Encoding =
        Encoding::new(prefix::APPLICATION_VND_POCKETLEARN);
    pub const APPLICATION_VND_POWERBUILDER6: Encoding =
        Encoding::new(prefix::APPLICATION_VND_POWERBUILDER6);
    pub const APPLICATION_VND_POWERBUILDER6_S: Encoding =
        Encoding::new(prefix::APPLICATION_VND_POWERBUILDER6_S);
    pub const APPLICATION_VND_POWERBUILDER7: Encoding =
        Encoding::new(prefix::APPLICATION_VND_POWERBUILDER7);
    pub const APPLICATION_VND_POWERBUILDER7_S: Encoding =
        Encoding::new(prefix::APPLICATION_VND_POWERBUILDER7_S);
    pub const APPLICATION_VND_POWERBUILDER75: Encoding =
        Encoding::new(prefix::APPLICATION_VND_POWERBUILDER75);
    pub const APPLICATION_VND_POWERBUILDER75_S: Encoding =
        Encoding::new(prefix::APPLICATION_VND_POWERBUILDER75_S);
    pub const APPLICATION_VND_PREMINET: Encoding = Encoding::new(prefix::APPLICATION_VND_PREMINET);
    pub const APPLICATION_VND_PREVIEWSYSTEMS_BOX: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PREVIEWSYSTEMS_BOX);
    pub const APPLICATION_VND_PROTEUS_MAGAZINE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PROTEUS_MAGAZINE);
    pub const APPLICATION_VND_PSFS: Encoding = Encoding::new(prefix::APPLICATION_VND_PSFS);
    pub const APPLICATION_VND_PT_MUNDUSMUNDI: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PT_MUNDUSMUNDI);
    pub const APPLICATION_VND_PUBLISHARE_DELTA_TREE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PUBLISHARE_DELTA_TREE);
    pub const APPLICATION_VND_PVI_PTID1: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PVI_PTID1);
    pub const APPLICATION_VND_PWG_MULTIPLEXED: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PWG_MULTIPLEXED);
    pub const APPLICATION_VND_PWG_XHTML_PRINT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_PWG_XHTML_PRINT_XML);
    pub const APPLICATION_VND_QUALCOMM_BREW_APP_RES: Encoding =
        Encoding::new(prefix::APPLICATION_VND_QUALCOMM_BREW_APP_RES);
    pub const APPLICATION_VND_QUARANTAINENET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_QUARANTAINENET);
    pub const APPLICATION_VND_QUOBJECT_QUOXDOCUMENT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_QUOBJECT_QUOXDOCUMENT);
    pub const APPLICATION_VND_RADISYS_MOML_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MOML_XML);
    pub const APPLICATION_VND_RADISYS_MSML_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_XML);
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_AUDIT_XML);
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_CONF_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_AUDIT_CONF_XML);
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_CONN_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_AUDIT_CONN_XML);
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_DIALOG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_AUDIT_DIALOG_XML);
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_STREAM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_AUDIT_STREAM_XML);
    pub const APPLICATION_VND_RADISYS_MSML_CONF_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_CONF_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_DIALOG_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_BASE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_DIALOG_BASE_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_DETECT_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_DETECT_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_SENDRECV_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_SENDRECV_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_GROUP_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_DIALOG_GROUP_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_SPEECH_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_DIALOG_SPEECH_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_TRANSFORM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RADISYS_MSML_DIALOG_TRANSFORM_XML);
    pub const APPLICATION_VND_RAINSTOR_DATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RAINSTOR_DATA);
    pub const APPLICATION_VND_RAPID: Encoding = Encoding::new(prefix::APPLICATION_VND_RAPID);
    pub const APPLICATION_VND_RAR: Encoding = Encoding::new(prefix::APPLICATION_VND_RAR);
    pub const APPLICATION_VND_REALVNC_BED: Encoding =
        Encoding::new(prefix::APPLICATION_VND_REALVNC_BED);
    pub const APPLICATION_VND_RECORDARE_MUSICXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RECORDARE_MUSICXML);
    pub const APPLICATION_VND_RECORDARE_MUSICXML_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RECORDARE_MUSICXML_XML);
    pub const APPLICATION_VND_RELPIPE: Encoding = Encoding::new(prefix::APPLICATION_VND_RELPIPE);
    pub const APPLICATION_VND_RESILIENT_LOGIC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RESILIENT_LOGIC);
    pub const APPLICATION_VND_RESTFUL_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RESTFUL_JSON);
    pub const APPLICATION_VND_RIG_CRYPTONOTE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RIG_CRYPTONOTE);
    pub const APPLICATION_VND_ROUTE66_LINK66_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ROUTE66_LINK66_XML);
    pub const APPLICATION_VND_RS_274X: Encoding = Encoding::new(prefix::APPLICATION_VND_RS_274X);
    pub const APPLICATION_VND_RUCKUS_DOWNLOAD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_RUCKUS_DOWNLOAD);
    pub const APPLICATION_VND_S3SMS: Encoding = Encoding::new(prefix::APPLICATION_VND_S3SMS);
    pub const APPLICATION_VND_SAILINGTRACKER_TRACK: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SAILINGTRACKER_TRACK);
    pub const APPLICATION_VND_SAR: Encoding = Encoding::new(prefix::APPLICATION_VND_SAR);
    pub const APPLICATION_VND_SBM_CID: Encoding = Encoding::new(prefix::APPLICATION_VND_SBM_CID);
    pub const APPLICATION_VND_SBM_MID2: Encoding = Encoding::new(prefix::APPLICATION_VND_SBM_MID2);
    pub const APPLICATION_VND_SCRIBUS: Encoding = Encoding::new(prefix::APPLICATION_VND_SCRIBUS);
    pub const APPLICATION_VND_SEALED_3DF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEALED_3DF);
    pub const APPLICATION_VND_SEALED_CSF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEALED_CSF);
    pub const APPLICATION_VND_SEALED_DOC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEALED_DOC);
    pub const APPLICATION_VND_SEALED_EML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEALED_EML);
    pub const APPLICATION_VND_SEALED_MHT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEALED_MHT);
    pub const APPLICATION_VND_SEALED_NET: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEALED_NET);
    pub const APPLICATION_VND_SEALED_PPT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEALED_PPT);
    pub const APPLICATION_VND_SEALED_TIFF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEALED_TIFF);
    pub const APPLICATION_VND_SEALED_XLS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEALED_XLS);
    pub const APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_HTML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_HTML);
    pub const APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_PDF: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_PDF);
    pub const APPLICATION_VND_SEEMAIL: Encoding = Encoding::new(prefix::APPLICATION_VND_SEEMAIL);
    pub const APPLICATION_VND_SEIS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SEIS_JSON);
    pub const APPLICATION_VND_SEMA: Encoding = Encoding::new(prefix::APPLICATION_VND_SEMA);
    pub const APPLICATION_VND_SEMD: Encoding = Encoding::new(prefix::APPLICATION_VND_SEMD);
    pub const APPLICATION_VND_SEMF: Encoding = Encoding::new(prefix::APPLICATION_VND_SEMF);
    pub const APPLICATION_VND_SHADE_SAVE_FILE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SHADE_SAVE_FILE);
    pub const APPLICATION_VND_SHANA_INFORMED_FORMDATA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SHANA_INFORMED_FORMDATA);
    pub const APPLICATION_VND_SHANA_INFORMED_FORMTEMPLATE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SHANA_INFORMED_FORMTEMPLATE);
    pub const APPLICATION_VND_SHANA_INFORMED_INTERCHANGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SHANA_INFORMED_INTERCHANGE);
    pub const APPLICATION_VND_SHANA_INFORMED_PACKAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SHANA_INFORMED_PACKAGE);
    pub const APPLICATION_VND_SHOOTPROOF_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SHOOTPROOF_JSON);
    pub const APPLICATION_VND_SHOPKICK_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SHOPKICK_JSON);
    pub const APPLICATION_VND_SHP: Encoding = Encoding::new(prefix::APPLICATION_VND_SHP);
    pub const APPLICATION_VND_SHX: Encoding = Encoding::new(prefix::APPLICATION_VND_SHX);
    pub const APPLICATION_VND_SIGROK_SESSION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SIGROK_SESSION);
    pub const APPLICATION_VND_SIREN_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SIREN_JSON);
    pub const APPLICATION_VND_SMAF: Encoding = Encoding::new(prefix::APPLICATION_VND_SMAF);
    pub const APPLICATION_VND_SMART_NOTEBOOK: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SMART_NOTEBOOK);
    pub const APPLICATION_VND_SMART_TEACHER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SMART_TEACHER);
    pub const APPLICATION_VND_SMINTIO_PORTALS_ARCHIVE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SMINTIO_PORTALS_ARCHIVE);
    pub const APPLICATION_VND_SNESDEV_PAGE_TABLE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SNESDEV_PAGE_TABLE);
    pub const APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML);
    pub const APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML_ZIP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML_ZIP);
    pub const APPLICATION_VND_SOLENT_SDKM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SOLENT_SDKM_XML);
    pub const APPLICATION_VND_SPOTFIRE_DXP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SPOTFIRE_DXP);
    pub const APPLICATION_VND_SPOTFIRE_SFS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SPOTFIRE_SFS);
    pub const APPLICATION_VND_SQLITE3: Encoding = Encoding::new(prefix::APPLICATION_VND_SQLITE3);
    pub const APPLICATION_VND_SSS_COD: Encoding = Encoding::new(prefix::APPLICATION_VND_SSS_COD);
    pub const APPLICATION_VND_SSS_DTF: Encoding = Encoding::new(prefix::APPLICATION_VND_SSS_DTF);
    pub const APPLICATION_VND_SSS_NTF: Encoding = Encoding::new(prefix::APPLICATION_VND_SSS_NTF);
    pub const APPLICATION_VND_STEPMANIA_PACKAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_STEPMANIA_PACKAGE);
    pub const APPLICATION_VND_STEPMANIA_STEPCHART: Encoding =
        Encoding::new(prefix::APPLICATION_VND_STEPMANIA_STEPCHART);
    pub const APPLICATION_VND_STREET_STREAM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_STREET_STREAM);
    pub const APPLICATION_VND_SUN_WADL_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SUN_WADL_XML);
    pub const APPLICATION_VND_SUS_CALENDAR: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SUS_CALENDAR);
    pub const APPLICATION_VND_SVD: Encoding = Encoding::new(prefix::APPLICATION_VND_SVD);
    pub const APPLICATION_VND_SWIFTVIEW_ICS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SWIFTVIEW_ICS);
    pub const APPLICATION_VND_SYBYL_MOL2: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYBYL_MOL2);
    pub const APPLICATION_VND_SYCLE_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYCLE_XML);
    pub const APPLICATION_VND_SYFT_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYFT_JSON);
    pub const APPLICATION_VND_SYNCML_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYNCML_XML);
    pub const APPLICATION_VND_SYNCML_DM_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYNCML_DM_WBXML);
    pub const APPLICATION_VND_SYNCML_DM_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYNCML_DM_XML);
    pub const APPLICATION_VND_SYNCML_DM_NOTIFICATION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYNCML_DM_NOTIFICATION);
    pub const APPLICATION_VND_SYNCML_DMDDF_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYNCML_DMDDF_WBXML);
    pub const APPLICATION_VND_SYNCML_DMDDF_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYNCML_DMDDF_XML);
    pub const APPLICATION_VND_SYNCML_DMTNDS_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYNCML_DMTNDS_WBXML);
    pub const APPLICATION_VND_SYNCML_DMTNDS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYNCML_DMTNDS_XML);
    pub const APPLICATION_VND_SYNCML_DS_NOTIFICATION: Encoding =
        Encoding::new(prefix::APPLICATION_VND_SYNCML_DS_NOTIFICATION);
    pub const APPLICATION_VND_TABLESCHEMA_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_TABLESCHEMA_JSON);
    pub const APPLICATION_VND_TAO_INTENT_MODULE_ARCHIVE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_TAO_INTENT_MODULE_ARCHIVE);
    pub const APPLICATION_VND_TCPDUMP_PCAP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_TCPDUMP_PCAP);
    pub const APPLICATION_VND_THINK_CELL_PPTTC_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_THINK_CELL_PPTTC_JSON);
    pub const APPLICATION_VND_TMD_MEDIAFLEX_API_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_TMD_MEDIAFLEX_API_XML);
    pub const APPLICATION_VND_TML: Encoding = Encoding::new(prefix::APPLICATION_VND_TML);
    pub const APPLICATION_VND_TMOBILE_LIVETV: Encoding =
        Encoding::new(prefix::APPLICATION_VND_TMOBILE_LIVETV);
    pub const APPLICATION_VND_TRI_ONESOURCE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_TRI_ONESOURCE);
    pub const APPLICATION_VND_TRID_TPT: Encoding = Encoding::new(prefix::APPLICATION_VND_TRID_TPT);
    pub const APPLICATION_VND_TRISCAPE_MXS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_TRISCAPE_MXS);
    pub const APPLICATION_VND_TRUEAPP: Encoding = Encoding::new(prefix::APPLICATION_VND_TRUEAPP);
    pub const APPLICATION_VND_TRUEDOC: Encoding = Encoding::new(prefix::APPLICATION_VND_TRUEDOC);
    pub const APPLICATION_VND_UBISOFT_WEBPLAYER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UBISOFT_WEBPLAYER);
    pub const APPLICATION_VND_UFDL: Encoding = Encoding::new(prefix::APPLICATION_VND_UFDL);
    pub const APPLICATION_VND_UIQ_THEME: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UIQ_THEME);
    pub const APPLICATION_VND_UMAJIN: Encoding = Encoding::new(prefix::APPLICATION_VND_UMAJIN);
    pub const APPLICATION_VND_UNITY: Encoding = Encoding::new(prefix::APPLICATION_VND_UNITY);
    pub const APPLICATION_VND_UOML_XML: Encoding = Encoding::new(prefix::APPLICATION_VND_UOML_XML);
    pub const APPLICATION_VND_UPLANET_ALERT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_ALERT);
    pub const APPLICATION_VND_UPLANET_ALERT_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_ALERT_WBXML);
    pub const APPLICATION_VND_UPLANET_BEARER_CHOICE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_BEARER_CHOICE);
    pub const APPLICATION_VND_UPLANET_BEARER_CHOICE_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_BEARER_CHOICE_WBXML);
    pub const APPLICATION_VND_UPLANET_CACHEOP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_CACHEOP);
    pub const APPLICATION_VND_UPLANET_CACHEOP_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_CACHEOP_WBXML);
    pub const APPLICATION_VND_UPLANET_CHANNEL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_CHANNEL);
    pub const APPLICATION_VND_UPLANET_CHANNEL_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_CHANNEL_WBXML);
    pub const APPLICATION_VND_UPLANET_LIST: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_LIST);
    pub const APPLICATION_VND_UPLANET_LIST_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_LIST_WBXML);
    pub const APPLICATION_VND_UPLANET_LISTCMD: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_LISTCMD);
    pub const APPLICATION_VND_UPLANET_LISTCMD_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_LISTCMD_WBXML);
    pub const APPLICATION_VND_UPLANET_SIGNAL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_UPLANET_SIGNAL);
    pub const APPLICATION_VND_URI_MAP: Encoding = Encoding::new(prefix::APPLICATION_VND_URI_MAP);
    pub const APPLICATION_VND_VALVE_SOURCE_MATERIAL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_VALVE_SOURCE_MATERIAL);
    pub const APPLICATION_VND_VCX: Encoding = Encoding::new(prefix::APPLICATION_VND_VCX);
    pub const APPLICATION_VND_VD_STUDY: Encoding = Encoding::new(prefix::APPLICATION_VND_VD_STUDY);
    pub const APPLICATION_VND_VECTORWORKS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_VECTORWORKS);
    pub const APPLICATION_VND_VEL_JSON: Encoding = Encoding::new(prefix::APPLICATION_VND_VEL_JSON);
    pub const APPLICATION_VND_VERIMATRIX_VCAS: Encoding =
        Encoding::new(prefix::APPLICATION_VND_VERIMATRIX_VCAS);
    pub const APPLICATION_VND_VERITONE_AION_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_VERITONE_AION_JSON);
    pub const APPLICATION_VND_VERYANT_THIN: Encoding =
        Encoding::new(prefix::APPLICATION_VND_VERYANT_THIN);
    pub const APPLICATION_VND_VES_ENCRYPTED: Encoding =
        Encoding::new(prefix::APPLICATION_VND_VES_ENCRYPTED);
    pub const APPLICATION_VND_VIDSOFT_VIDCONFERENCE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_VIDSOFT_VIDCONFERENCE);
    pub const APPLICATION_VND_VISIO: Encoding = Encoding::new(prefix::APPLICATION_VND_VISIO);
    pub const APPLICATION_VND_VISIONARY: Encoding =
        Encoding::new(prefix::APPLICATION_VND_VISIONARY);
    pub const APPLICATION_VND_VIVIDENCE_SCRIPTFILE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_VIVIDENCE_SCRIPTFILE);
    pub const APPLICATION_VND_VSF: Encoding = Encoding::new(prefix::APPLICATION_VND_VSF);
    pub const APPLICATION_VND_WAP_SIC: Encoding = Encoding::new(prefix::APPLICATION_VND_WAP_SIC);
    pub const APPLICATION_VND_WAP_SLC: Encoding = Encoding::new(prefix::APPLICATION_VND_WAP_SLC);
    pub const APPLICATION_VND_WAP_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WAP_WBXML);
    pub const APPLICATION_VND_WAP_WMLC: Encoding = Encoding::new(prefix::APPLICATION_VND_WAP_WMLC);
    pub const APPLICATION_VND_WAP_WMLSCRIPTC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WAP_WMLSCRIPTC);
    pub const APPLICATION_VND_WASMFLOW_WAFL: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WASMFLOW_WAFL);
    pub const APPLICATION_VND_WEBTURBO: Encoding = Encoding::new(prefix::APPLICATION_VND_WEBTURBO);
    pub const APPLICATION_VND_WFA_DPP: Encoding = Encoding::new(prefix::APPLICATION_VND_WFA_DPP);
    pub const APPLICATION_VND_WFA_P2P: Encoding = Encoding::new(prefix::APPLICATION_VND_WFA_P2P);
    pub const APPLICATION_VND_WFA_WSC: Encoding = Encoding::new(prefix::APPLICATION_VND_WFA_WSC);
    pub const APPLICATION_VND_WINDOWS_DEVICEPAIRING: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WINDOWS_DEVICEPAIRING);
    pub const APPLICATION_VND_WMC: Encoding = Encoding::new(prefix::APPLICATION_VND_WMC);
    pub const APPLICATION_VND_WMF_BOOTSTRAP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WMF_BOOTSTRAP);
    pub const APPLICATION_VND_WOLFRAM_MATHEMATICA: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WOLFRAM_MATHEMATICA);
    pub const APPLICATION_VND_WOLFRAM_MATHEMATICA_PACKAGE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WOLFRAM_MATHEMATICA_PACKAGE);
    pub const APPLICATION_VND_WOLFRAM_PLAYER: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WOLFRAM_PLAYER);
    pub const APPLICATION_VND_WORDLIFT: Encoding = Encoding::new(prefix::APPLICATION_VND_WORDLIFT);
    pub const APPLICATION_VND_WORDPERFECT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WORDPERFECT);
    pub const APPLICATION_VND_WQD: Encoding = Encoding::new(prefix::APPLICATION_VND_WQD);
    pub const APPLICATION_VND_WRQ_HP3000_LABELLED: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WRQ_HP3000_LABELLED);
    pub const APPLICATION_VND_WT_STF: Encoding = Encoding::new(prefix::APPLICATION_VND_WT_STF);
    pub const APPLICATION_VND_WV_CSP_WBXML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WV_CSP_WBXML);
    pub const APPLICATION_VND_WV_CSP_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WV_CSP_XML);
    pub const APPLICATION_VND_WV_SSP_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_WV_SSP_XML);
    pub const APPLICATION_VND_XACML_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VND_XACML_JSON);
    pub const APPLICATION_VND_XARA: Encoding = Encoding::new(prefix::APPLICATION_VND_XARA);
    pub const APPLICATION_VND_XECRETS_ENCRYPTED: Encoding =
        Encoding::new(prefix::APPLICATION_VND_XECRETS_ENCRYPTED);
    pub const APPLICATION_VND_XFDL: Encoding = Encoding::new(prefix::APPLICATION_VND_XFDL);
    pub const APPLICATION_VND_XFDL_WEBFORM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_XFDL_WEBFORM);
    pub const APPLICATION_VND_XMI_XML: Encoding = Encoding::new(prefix::APPLICATION_VND_XMI_XML);
    pub const APPLICATION_VND_XMPIE_CPKG: Encoding =
        Encoding::new(prefix::APPLICATION_VND_XMPIE_CPKG);
    pub const APPLICATION_VND_XMPIE_DPKG: Encoding =
        Encoding::new(prefix::APPLICATION_VND_XMPIE_DPKG);
    pub const APPLICATION_VND_XMPIE_PLAN: Encoding =
        Encoding::new(prefix::APPLICATION_VND_XMPIE_PLAN);
    pub const APPLICATION_VND_XMPIE_PPKG: Encoding =
        Encoding::new(prefix::APPLICATION_VND_XMPIE_PPKG);
    pub const APPLICATION_VND_XMPIE_XLIM: Encoding =
        Encoding::new(prefix::APPLICATION_VND_XMPIE_XLIM);
    pub const APPLICATION_VND_YAMAHA_HV_DIC: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YAMAHA_HV_DIC);
    pub const APPLICATION_VND_YAMAHA_HV_SCRIPT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YAMAHA_HV_SCRIPT);
    pub const APPLICATION_VND_YAMAHA_HV_VOICE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YAMAHA_HV_VOICE);
    pub const APPLICATION_VND_YAMAHA_OPENSCOREFORMAT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YAMAHA_OPENSCOREFORMAT);
    pub const APPLICATION_VND_YAMAHA_OPENSCOREFORMAT_OSFPVG_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YAMAHA_OPENSCOREFORMAT_OSFPVG_XML);
    pub const APPLICATION_VND_YAMAHA_REMOTE_SETUP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YAMAHA_REMOTE_SETUP);
    pub const APPLICATION_VND_YAMAHA_SMAF_AUDIO: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YAMAHA_SMAF_AUDIO);
    pub const APPLICATION_VND_YAMAHA_SMAF_PHRASE: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YAMAHA_SMAF_PHRASE);
    pub const APPLICATION_VND_YAMAHA_THROUGH_NGN: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YAMAHA_THROUGH_NGN);
    pub const APPLICATION_VND_YAMAHA_TUNNEL_UDPENCAP: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YAMAHA_TUNNEL_UDPENCAP);
    pub const APPLICATION_VND_YAOWEME: Encoding = Encoding::new(prefix::APPLICATION_VND_YAOWEME);
    pub const APPLICATION_VND_YELLOWRIVER_CUSTOM_MENU: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YELLOWRIVER_CUSTOM_MENU);
    pub const APPLICATION_VND_YOUTUBE_YT: Encoding =
        Encoding::new(prefix::APPLICATION_VND_YOUTUBE_YT);
    pub const APPLICATION_VND_ZUL: Encoding = Encoding::new(prefix::APPLICATION_VND_ZUL);
    pub const APPLICATION_VND_ZZAZZ_DECK_XML: Encoding =
        Encoding::new(prefix::APPLICATION_VND_ZZAZZ_DECK_XML);
    pub const APPLICATION_VOICEXML_XML: Encoding = Encoding::new(prefix::APPLICATION_VOICEXML_XML);
    pub const APPLICATION_VOUCHER_CMS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_VOUCHER_CMS_JSON);
    pub const APPLICATION_VQ_RTCPXR: Encoding = Encoding::new(prefix::APPLICATION_VQ_RTCPXR);
    pub const APPLICATION_WASM: Encoding = Encoding::new(prefix::APPLICATION_WASM);
    pub const APPLICATION_WATCHERINFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_WATCHERINFO_XML);
    pub const APPLICATION_WEBPUSH_OPTIONS_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_WEBPUSH_OPTIONS_JSON);
    pub const APPLICATION_WHOISPP_QUERY: Encoding =
        Encoding::new(prefix::APPLICATION_WHOISPP_QUERY);
    pub const APPLICATION_WHOISPP_RESPONSE: Encoding =
        Encoding::new(prefix::APPLICATION_WHOISPP_RESPONSE);
    pub const APPLICATION_WIDGET: Encoding = Encoding::new(prefix::APPLICATION_WIDGET);
    pub const APPLICATION_WITA: Encoding = Encoding::new(prefix::APPLICATION_WITA);
    pub const APPLICATION_WORDPERFECT5_1: Encoding =
        Encoding::new(prefix::APPLICATION_WORDPERFECT5_1);
    pub const APPLICATION_WSDL_XML: Encoding = Encoding::new(prefix::APPLICATION_WSDL_XML);
    pub const APPLICATION_WSPOLICY_XML: Encoding = Encoding::new(prefix::APPLICATION_WSPOLICY_XML);
    pub const APPLICATION_X_PKI_MESSAGE: Encoding =
        Encoding::new(prefix::APPLICATION_X_PKI_MESSAGE);
    pub const APPLICATION_X_WWW_FORM_URLENCODED: Encoding =
        Encoding::new(prefix::APPLICATION_X_WWW_FORM_URLENCODED);
    pub const APPLICATION_X_X509_CA_CERT: Encoding =
        Encoding::new(prefix::APPLICATION_X_X509_CA_CERT);
    pub const APPLICATION_X_X509_CA_RA_CERT: Encoding =
        Encoding::new(prefix::APPLICATION_X_X509_CA_RA_CERT);
    pub const APPLICATION_X_X509_NEXT_CA_CERT: Encoding =
        Encoding::new(prefix::APPLICATION_X_X509_NEXT_CA_CERT);
    pub const APPLICATION_X400_BP: Encoding = Encoding::new(prefix::APPLICATION_X400_BP);
    pub const APPLICATION_XACML_XML: Encoding = Encoding::new(prefix::APPLICATION_XACML_XML);
    pub const APPLICATION_XCAP_ATT_XML: Encoding = Encoding::new(prefix::APPLICATION_XCAP_ATT_XML);
    pub const APPLICATION_XCAP_CAPS_XML: Encoding =
        Encoding::new(prefix::APPLICATION_XCAP_CAPS_XML);
    pub const APPLICATION_XCAP_DIFF_XML: Encoding =
        Encoding::new(prefix::APPLICATION_XCAP_DIFF_XML);
    pub const APPLICATION_XCAP_EL_XML: Encoding = Encoding::new(prefix::APPLICATION_XCAP_EL_XML);
    pub const APPLICATION_XCAP_ERROR_XML: Encoding =
        Encoding::new(prefix::APPLICATION_XCAP_ERROR_XML);
    pub const APPLICATION_XCAP_NS_XML: Encoding = Encoding::new(prefix::APPLICATION_XCAP_NS_XML);
    pub const APPLICATION_XCON_CONFERENCE_INFO_XML: Encoding =
        Encoding::new(prefix::APPLICATION_XCON_CONFERENCE_INFO_XML);
    pub const APPLICATION_XCON_CONFERENCE_INFO_DIFF_XML: Encoding =
        Encoding::new(prefix::APPLICATION_XCON_CONFERENCE_INFO_DIFF_XML);
    pub const APPLICATION_XENC_XML: Encoding = Encoding::new(prefix::APPLICATION_XENC_XML);
    pub const APPLICATION_XFDF: Encoding = Encoding::new(prefix::APPLICATION_XFDF);
    pub const APPLICATION_XHTML_XML: Encoding = Encoding::new(prefix::APPLICATION_XHTML_XML);
    pub const APPLICATION_XLIFF_XML: Encoding = Encoding::new(prefix::APPLICATION_XLIFF_XML);
    pub const APPLICATION_XML: Encoding = Encoding::new(prefix::APPLICATION_XML);
    pub const APPLICATION_XML_DTD: Encoding = Encoding::new(prefix::APPLICATION_XML_DTD);
    pub const APPLICATION_XML_EXTERNAL_PARSED_ENTITY: Encoding =
        Encoding::new(prefix::APPLICATION_XML_EXTERNAL_PARSED_ENTITY);
    pub const APPLICATION_XML_PATCH_XML: Encoding =
        Encoding::new(prefix::APPLICATION_XML_PATCH_XML);
    pub const APPLICATION_XMPP_XML: Encoding = Encoding::new(prefix::APPLICATION_XMPP_XML);
    pub const APPLICATION_XOP_XML: Encoding = Encoding::new(prefix::APPLICATION_XOP_XML);
    pub const APPLICATION_XSLT_XML: Encoding = Encoding::new(prefix::APPLICATION_XSLT_XML);
    pub const APPLICATION_XV_XML: Encoding = Encoding::new(prefix::APPLICATION_XV_XML);
    pub const APPLICATION_YAML: Encoding = Encoding::new(prefix::APPLICATION_YAML);
    pub const APPLICATION_YANG: Encoding = Encoding::new(prefix::APPLICATION_YANG);
    pub const APPLICATION_YANG_DATA_CBOR: Encoding =
        Encoding::new(prefix::APPLICATION_YANG_DATA_CBOR);
    pub const APPLICATION_YANG_DATA_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_YANG_DATA_JSON);
    pub const APPLICATION_YANG_DATA_XML: Encoding =
        Encoding::new(prefix::APPLICATION_YANG_DATA_XML);
    pub const APPLICATION_YANG_PATCH_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_YANG_PATCH_JSON);
    pub const APPLICATION_YANG_PATCH_XML: Encoding =
        Encoding::new(prefix::APPLICATION_YANG_PATCH_XML);
    pub const APPLICATION_YANG_SID_JSON: Encoding =
        Encoding::new(prefix::APPLICATION_YANG_SID_JSON);
    pub const APPLICATION_YIN_XML: Encoding = Encoding::new(prefix::APPLICATION_YIN_XML);
    pub const APPLICATION_ZIP: Encoding = Encoding::new(prefix::APPLICATION_ZIP);
    pub const APPLICATION_ZLIB: Encoding = Encoding::new(prefix::APPLICATION_ZLIB);
    pub const APPLICATION_ZSTD: Encoding = Encoding::new(prefix::APPLICATION_ZSTD);
    pub const AUDIO_1D_INTERLEAVED_PARITYFEC: Encoding =
        Encoding::new(prefix::AUDIO_1D_INTERLEAVED_PARITYFEC);
    pub const AUDIO_32KADPCM: Encoding = Encoding::new(prefix::AUDIO_32KADPCM);
    pub const AUDIO_3GPP: Encoding = Encoding::new(prefix::AUDIO_3GPP);
    pub const AUDIO_3GPP2: Encoding = Encoding::new(prefix::AUDIO_3GPP2);
    pub const AUDIO_AMR: Encoding = Encoding::new(prefix::AUDIO_AMR);
    pub const AUDIO_AMR_WB: Encoding = Encoding::new(prefix::AUDIO_AMR_WB);
    pub const AUDIO_ATRAC_ADVANCED_LOSSLESS: Encoding =
        Encoding::new(prefix::AUDIO_ATRAC_ADVANCED_LOSSLESS);
    pub const AUDIO_ATRAC_X: Encoding = Encoding::new(prefix::AUDIO_ATRAC_X);
    pub const AUDIO_ATRAC3: Encoding = Encoding::new(prefix::AUDIO_ATRAC3);
    pub const AUDIO_BV16: Encoding = Encoding::new(prefix::AUDIO_BV16);
    pub const AUDIO_BV32: Encoding = Encoding::new(prefix::AUDIO_BV32);
    pub const AUDIO_CN: Encoding = Encoding::new(prefix::AUDIO_CN);
    pub const AUDIO_DAT12: Encoding = Encoding::new(prefix::AUDIO_DAT12);
    pub const AUDIO_DV: Encoding = Encoding::new(prefix::AUDIO_DV);
    pub const AUDIO_DVI4: Encoding = Encoding::new(prefix::AUDIO_DVI4);
    pub const AUDIO_EVRC: Encoding = Encoding::new(prefix::AUDIO_EVRC);
    pub const AUDIO_EVRC_QCP: Encoding = Encoding::new(prefix::AUDIO_EVRC_QCP);
    pub const AUDIO_EVRC0: Encoding = Encoding::new(prefix::AUDIO_EVRC0);
    pub const AUDIO_EVRC1: Encoding = Encoding::new(prefix::AUDIO_EVRC1);
    pub const AUDIO_EVRCB: Encoding = Encoding::new(prefix::AUDIO_EVRCB);
    pub const AUDIO_EVRCB0: Encoding = Encoding::new(prefix::AUDIO_EVRCB0);
    pub const AUDIO_EVRCB1: Encoding = Encoding::new(prefix::AUDIO_EVRCB1);
    pub const AUDIO_EVRCNW: Encoding = Encoding::new(prefix::AUDIO_EVRCNW);
    pub const AUDIO_EVRCNW0: Encoding = Encoding::new(prefix::AUDIO_EVRCNW0);
    pub const AUDIO_EVRCNW1: Encoding = Encoding::new(prefix::AUDIO_EVRCNW1);
    pub const AUDIO_EVRCWB: Encoding = Encoding::new(prefix::AUDIO_EVRCWB);
    pub const AUDIO_EVRCWB0: Encoding = Encoding::new(prefix::AUDIO_EVRCWB0);
    pub const AUDIO_EVRCWB1: Encoding = Encoding::new(prefix::AUDIO_EVRCWB1);
    pub const AUDIO_EVS: Encoding = Encoding::new(prefix::AUDIO_EVS);
    pub const AUDIO_G711_0: Encoding = Encoding::new(prefix::AUDIO_G711_0);
    pub const AUDIO_G719: Encoding = Encoding::new(prefix::AUDIO_G719);
    pub const AUDIO_G722: Encoding = Encoding::new(prefix::AUDIO_G722);
    pub const AUDIO_G7221: Encoding = Encoding::new(prefix::AUDIO_G7221);
    pub const AUDIO_G723: Encoding = Encoding::new(prefix::AUDIO_G723);
    pub const AUDIO_G726_16: Encoding = Encoding::new(prefix::AUDIO_G726_16);
    pub const AUDIO_G726_24: Encoding = Encoding::new(prefix::AUDIO_G726_24);
    pub const AUDIO_G726_32: Encoding = Encoding::new(prefix::AUDIO_G726_32);
    pub const AUDIO_G726_40: Encoding = Encoding::new(prefix::AUDIO_G726_40);
    pub const AUDIO_G728: Encoding = Encoding::new(prefix::AUDIO_G728);
    pub const AUDIO_G729: Encoding = Encoding::new(prefix::AUDIO_G729);
    pub const AUDIO_G7291: Encoding = Encoding::new(prefix::AUDIO_G7291);
    pub const AUDIO_G729D: Encoding = Encoding::new(prefix::AUDIO_G729D);
    pub const AUDIO_G729E: Encoding = Encoding::new(prefix::AUDIO_G729E);
    pub const AUDIO_GSM: Encoding = Encoding::new(prefix::AUDIO_GSM);
    pub const AUDIO_GSM_EFR: Encoding = Encoding::new(prefix::AUDIO_GSM_EFR);
    pub const AUDIO_GSM_HR_08: Encoding = Encoding::new(prefix::AUDIO_GSM_HR_08);
    pub const AUDIO_L16: Encoding = Encoding::new(prefix::AUDIO_L16);
    pub const AUDIO_L20: Encoding = Encoding::new(prefix::AUDIO_L20);
    pub const AUDIO_L24: Encoding = Encoding::new(prefix::AUDIO_L24);
    pub const AUDIO_L8: Encoding = Encoding::new(prefix::AUDIO_L8);
    pub const AUDIO_LPC: Encoding = Encoding::new(prefix::AUDIO_LPC);
    pub const AUDIO_MELP: Encoding = Encoding::new(prefix::AUDIO_MELP);
    pub const AUDIO_MELP1200: Encoding = Encoding::new(prefix::AUDIO_MELP1200);
    pub const AUDIO_MELP2400: Encoding = Encoding::new(prefix::AUDIO_MELP2400);
    pub const AUDIO_MELP600: Encoding = Encoding::new(prefix::AUDIO_MELP600);
    pub const AUDIO_MP4A_LATM: Encoding = Encoding::new(prefix::AUDIO_MP4A_LATM);
    pub const AUDIO_MPA: Encoding = Encoding::new(prefix::AUDIO_MPA);
    pub const AUDIO_PCMA: Encoding = Encoding::new(prefix::AUDIO_PCMA);
    pub const AUDIO_PCMA_WB: Encoding = Encoding::new(prefix::AUDIO_PCMA_WB);
    pub const AUDIO_PCMU: Encoding = Encoding::new(prefix::AUDIO_PCMU);
    pub const AUDIO_PCMU_WB: Encoding = Encoding::new(prefix::AUDIO_PCMU_WB);
    pub const AUDIO_QCELP: Encoding = Encoding::new(prefix::AUDIO_QCELP);
    pub const AUDIO_RED: Encoding = Encoding::new(prefix::AUDIO_RED);
    pub const AUDIO_SMV: Encoding = Encoding::new(prefix::AUDIO_SMV);
    pub const AUDIO_SMV_QCP: Encoding = Encoding::new(prefix::AUDIO_SMV_QCP);
    pub const AUDIO_SMV0: Encoding = Encoding::new(prefix::AUDIO_SMV0);
    pub const AUDIO_TETRA_ACELP: Encoding = Encoding::new(prefix::AUDIO_TETRA_ACELP);
    pub const AUDIO_TETRA_ACELP_BB: Encoding = Encoding::new(prefix::AUDIO_TETRA_ACELP_BB);
    pub const AUDIO_TSVCIS: Encoding = Encoding::new(prefix::AUDIO_TSVCIS);
    pub const AUDIO_UEMCLIP: Encoding = Encoding::new(prefix::AUDIO_UEMCLIP);
    pub const AUDIO_VDVI: Encoding = Encoding::new(prefix::AUDIO_VDVI);
    pub const AUDIO_VMR_WB: Encoding = Encoding::new(prefix::AUDIO_VMR_WB);
    pub const AUDIO_AAC: Encoding = Encoding::new(prefix::AUDIO_AAC);
    pub const AUDIO_AC3: Encoding = Encoding::new(prefix::AUDIO_AC3);
    pub const AUDIO_AMR_WB_P: Encoding = Encoding::new(prefix::AUDIO_AMR_WB_P);
    pub const AUDIO_APTX: Encoding = Encoding::new(prefix::AUDIO_APTX);
    pub const AUDIO_ASC: Encoding = Encoding::new(prefix::AUDIO_ASC);
    pub const AUDIO_BASIC: Encoding = Encoding::new(prefix::AUDIO_BASIC);
    pub const AUDIO_CLEARMODE: Encoding = Encoding::new(prefix::AUDIO_CLEARMODE);
    pub const AUDIO_DLS: Encoding = Encoding::new(prefix::AUDIO_DLS);
    pub const AUDIO_DSR_ES201108: Encoding = Encoding::new(prefix::AUDIO_DSR_ES201108);
    pub const AUDIO_DSR_ES202050: Encoding = Encoding::new(prefix::AUDIO_DSR_ES202050);
    pub const AUDIO_DSR_ES202211: Encoding = Encoding::new(prefix::AUDIO_DSR_ES202211);
    pub const AUDIO_DSR_ES202212: Encoding = Encoding::new(prefix::AUDIO_DSR_ES202212);
    pub const AUDIO_EAC3: Encoding = Encoding::new(prefix::AUDIO_EAC3);
    pub const AUDIO_ENCAPRTP: Encoding = Encoding::new(prefix::AUDIO_ENCAPRTP);
    pub const AUDIO_EXAMPLE: Encoding = Encoding::new(prefix::AUDIO_EXAMPLE);
    pub const AUDIO_FLEXFEC: Encoding = Encoding::new(prefix::AUDIO_FLEXFEC);
    pub const AUDIO_FWDRED: Encoding = Encoding::new(prefix::AUDIO_FWDRED);
    pub const AUDIO_ILBC: Encoding = Encoding::new(prefix::AUDIO_ILBC);
    pub const AUDIO_IP_MR_V2_5: Encoding = Encoding::new(prefix::AUDIO_IP_MR_V2_5);
    pub const AUDIO_MATROSKA: Encoding = Encoding::new(prefix::AUDIO_MATROSKA);
    pub const AUDIO_MHAS: Encoding = Encoding::new(prefix::AUDIO_MHAS);
    pub const AUDIO_MOBILE_XMF: Encoding = Encoding::new(prefix::AUDIO_MOBILE_XMF);
    pub const AUDIO_MP4: Encoding = Encoding::new(prefix::AUDIO_MP4);
    pub const AUDIO_MPA_ROBUST: Encoding = Encoding::new(prefix::AUDIO_MPA_ROBUST);
    pub const AUDIO_MPEG: Encoding = Encoding::new(prefix::AUDIO_MPEG);
    pub const AUDIO_MPEG4_GENERIC: Encoding = Encoding::new(prefix::AUDIO_MPEG4_GENERIC);
    pub const AUDIO_OGG: Encoding = Encoding::new(prefix::AUDIO_OGG);
    pub const AUDIO_OPUS: Encoding = Encoding::new(prefix::AUDIO_OPUS);
    pub const AUDIO_PARITYFEC: Encoding = Encoding::new(prefix::AUDIO_PARITYFEC);
    pub const AUDIO_PRS_SID: Encoding = Encoding::new(prefix::AUDIO_PRS_SID);
    pub const AUDIO_RAPTORFEC: Encoding = Encoding::new(prefix::AUDIO_RAPTORFEC);
    pub const AUDIO_RTP_ENC_AESCM128: Encoding = Encoding::new(prefix::AUDIO_RTP_ENC_AESCM128);
    pub const AUDIO_RTP_MIDI: Encoding = Encoding::new(prefix::AUDIO_RTP_MIDI);
    pub const AUDIO_RTPLOOPBACK: Encoding = Encoding::new(prefix::AUDIO_RTPLOOPBACK);
    pub const AUDIO_RTX: Encoding = Encoding::new(prefix::AUDIO_RTX);
    pub const AUDIO_SCIP: Encoding = Encoding::new(prefix::AUDIO_SCIP);
    pub const AUDIO_SOFA: Encoding = Encoding::new(prefix::AUDIO_SOFA);
    pub const AUDIO_SP_MIDI: Encoding = Encoding::new(prefix::AUDIO_SP_MIDI);
    pub const AUDIO_SPEEX: Encoding = Encoding::new(prefix::AUDIO_SPEEX);
    pub const AUDIO_T140C: Encoding = Encoding::new(prefix::AUDIO_T140C);
    pub const AUDIO_T38: Encoding = Encoding::new(prefix::AUDIO_T38);
    pub const AUDIO_TELEPHONE_EVENT: Encoding = Encoding::new(prefix::AUDIO_TELEPHONE_EVENT);
    pub const AUDIO_TONE: Encoding = Encoding::new(prefix::AUDIO_TONE);
    pub const AUDIO_ULPFEC: Encoding = Encoding::new(prefix::AUDIO_ULPFEC);
    pub const AUDIO_USAC: Encoding = Encoding::new(prefix::AUDIO_USAC);
    pub const AUDIO_VND_3GPP_IUFP: Encoding = Encoding::new(prefix::AUDIO_VND_3GPP_IUFP);
    pub const AUDIO_VND_4SB: Encoding = Encoding::new(prefix::AUDIO_VND_4SB);
    pub const AUDIO_VND_CELP: Encoding = Encoding::new(prefix::AUDIO_VND_CELP);
    pub const AUDIO_VND_AUDIOKOZ: Encoding = Encoding::new(prefix::AUDIO_VND_AUDIOKOZ);
    pub const AUDIO_VND_CISCO_NSE: Encoding = Encoding::new(prefix::AUDIO_VND_CISCO_NSE);
    pub const AUDIO_VND_CMLES_RADIO_EVENTS: Encoding =
        Encoding::new(prefix::AUDIO_VND_CMLES_RADIO_EVENTS);
    pub const AUDIO_VND_CNS_ANP1: Encoding = Encoding::new(prefix::AUDIO_VND_CNS_ANP1);
    pub const AUDIO_VND_CNS_INF1: Encoding = Encoding::new(prefix::AUDIO_VND_CNS_INF1);
    pub const AUDIO_VND_DECE_AUDIO: Encoding = Encoding::new(prefix::AUDIO_VND_DECE_AUDIO);
    pub const AUDIO_VND_DIGITAL_WINDS: Encoding = Encoding::new(prefix::AUDIO_VND_DIGITAL_WINDS);
    pub const AUDIO_VND_DLNA_ADTS: Encoding = Encoding::new(prefix::AUDIO_VND_DLNA_ADTS);
    pub const AUDIO_VND_DOLBY_HEAAC_1: Encoding = Encoding::new(prefix::AUDIO_VND_DOLBY_HEAAC_1);
    pub const AUDIO_VND_DOLBY_HEAAC_2: Encoding = Encoding::new(prefix::AUDIO_VND_DOLBY_HEAAC_2);
    pub const AUDIO_VND_DOLBY_MLP: Encoding = Encoding::new(prefix::AUDIO_VND_DOLBY_MLP);
    pub const AUDIO_VND_DOLBY_MPS: Encoding = Encoding::new(prefix::AUDIO_VND_DOLBY_MPS);
    pub const AUDIO_VND_DOLBY_PL2: Encoding = Encoding::new(prefix::AUDIO_VND_DOLBY_PL2);
    pub const AUDIO_VND_DOLBY_PL2X: Encoding = Encoding::new(prefix::AUDIO_VND_DOLBY_PL2X);
    pub const AUDIO_VND_DOLBY_PL2Z: Encoding = Encoding::new(prefix::AUDIO_VND_DOLBY_PL2Z);
    pub const AUDIO_VND_DOLBY_PULSE_1: Encoding = Encoding::new(prefix::AUDIO_VND_DOLBY_PULSE_1);
    pub const AUDIO_VND_DRA: Encoding = Encoding::new(prefix::AUDIO_VND_DRA);
    pub const AUDIO_VND_DTS: Encoding = Encoding::new(prefix::AUDIO_VND_DTS);
    pub const AUDIO_VND_DTS_HD: Encoding = Encoding::new(prefix::AUDIO_VND_DTS_HD);
    pub const AUDIO_VND_DTS_UHD: Encoding = Encoding::new(prefix::AUDIO_VND_DTS_UHD);
    pub const AUDIO_VND_DVB_FILE: Encoding = Encoding::new(prefix::AUDIO_VND_DVB_FILE);
    pub const AUDIO_VND_EVERAD_PLJ: Encoding = Encoding::new(prefix::AUDIO_VND_EVERAD_PLJ);
    pub const AUDIO_VND_HNS_AUDIO: Encoding = Encoding::new(prefix::AUDIO_VND_HNS_AUDIO);
    pub const AUDIO_VND_LUCENT_VOICE: Encoding = Encoding::new(prefix::AUDIO_VND_LUCENT_VOICE);
    pub const AUDIO_VND_MS_PLAYREADY_MEDIA_PYA: Encoding =
        Encoding::new(prefix::AUDIO_VND_MS_PLAYREADY_MEDIA_PYA);
    pub const AUDIO_VND_NOKIA_MOBILE_XMF: Encoding =
        Encoding::new(prefix::AUDIO_VND_NOKIA_MOBILE_XMF);
    pub const AUDIO_VND_NORTEL_VBK: Encoding = Encoding::new(prefix::AUDIO_VND_NORTEL_VBK);
    pub const AUDIO_VND_NUERA_ECELP4800: Encoding =
        Encoding::new(prefix::AUDIO_VND_NUERA_ECELP4800);
    pub const AUDIO_VND_NUERA_ECELP7470: Encoding =
        Encoding::new(prefix::AUDIO_VND_NUERA_ECELP7470);
    pub const AUDIO_VND_NUERA_ECELP9600: Encoding =
        Encoding::new(prefix::AUDIO_VND_NUERA_ECELP9600);
    pub const AUDIO_VND_OCTEL_SBC: Encoding = Encoding::new(prefix::AUDIO_VND_OCTEL_SBC);
    pub const AUDIO_VND_PRESONUS_MULTITRACK: Encoding =
        Encoding::new(prefix::AUDIO_VND_PRESONUS_MULTITRACK);
    pub const AUDIO_VND_QCELP: Encoding = Encoding::new(prefix::AUDIO_VND_QCELP);
    pub const AUDIO_VND_RHETOREX_32KADPCM: Encoding =
        Encoding::new(prefix::AUDIO_VND_RHETOREX_32KADPCM);
    pub const AUDIO_VND_RIP: Encoding = Encoding::new(prefix::AUDIO_VND_RIP);
    pub const AUDIO_VND_SEALEDMEDIA_SOFTSEAL_MPEG: Encoding =
        Encoding::new(prefix::AUDIO_VND_SEALEDMEDIA_SOFTSEAL_MPEG);
    pub const AUDIO_VND_VMX_CVSD: Encoding = Encoding::new(prefix::AUDIO_VND_VMX_CVSD);
    pub const AUDIO_VORBIS: Encoding = Encoding::new(prefix::AUDIO_VORBIS);
    pub const AUDIO_VORBIS_CONFIG: Encoding = Encoding::new(prefix::AUDIO_VORBIS_CONFIG);
    pub const FONT_COLLECTION: Encoding = Encoding::new(prefix::FONT_COLLECTION);
    pub const FONT_OTF: Encoding = Encoding::new(prefix::FONT_OTF);
    pub const FONT_SFNT: Encoding = Encoding::new(prefix::FONT_SFNT);
    pub const FONT_TTF: Encoding = Encoding::new(prefix::FONT_TTF);
    pub const FONT_WOFF: Encoding = Encoding::new(prefix::FONT_WOFF);
    pub const FONT_WOFF2: Encoding = Encoding::new(prefix::FONT_WOFF2);
    pub const IMAGE_ACES: Encoding = Encoding::new(prefix::IMAGE_ACES);
    pub const IMAGE_APNG: Encoding = Encoding::new(prefix::IMAGE_APNG);
    pub const IMAGE_AVCI: Encoding = Encoding::new(prefix::IMAGE_AVCI);
    pub const IMAGE_AVCS: Encoding = Encoding::new(prefix::IMAGE_AVCS);
    pub const IMAGE_AVIF: Encoding = Encoding::new(prefix::IMAGE_AVIF);
    pub const IMAGE_BMP: Encoding = Encoding::new(prefix::IMAGE_BMP);
    pub const IMAGE_CGM: Encoding = Encoding::new(prefix::IMAGE_CGM);
    pub const IMAGE_DICOM_RLE: Encoding = Encoding::new(prefix::IMAGE_DICOM_RLE);
    pub const IMAGE_DPX: Encoding = Encoding::new(prefix::IMAGE_DPX);
    pub const IMAGE_EMF: Encoding = Encoding::new(prefix::IMAGE_EMF);
    pub const IMAGE_EXAMPLE: Encoding = Encoding::new(prefix::IMAGE_EXAMPLE);
    pub const IMAGE_FITS: Encoding = Encoding::new(prefix::IMAGE_FITS);
    pub const IMAGE_G3FAX: Encoding = Encoding::new(prefix::IMAGE_G3FAX);
    pub const IMAGE_GIF: Encoding = Encoding::new(prefix::IMAGE_GIF);
    pub const IMAGE_HEIC: Encoding = Encoding::new(prefix::IMAGE_HEIC);
    pub const IMAGE_HEIC_SEQUENCE: Encoding = Encoding::new(prefix::IMAGE_HEIC_SEQUENCE);
    pub const IMAGE_HEIF: Encoding = Encoding::new(prefix::IMAGE_HEIF);
    pub const IMAGE_HEIF_SEQUENCE: Encoding = Encoding::new(prefix::IMAGE_HEIF_SEQUENCE);
    pub const IMAGE_HEJ2K: Encoding = Encoding::new(prefix::IMAGE_HEJ2K);
    pub const IMAGE_HSJ2: Encoding = Encoding::new(prefix::IMAGE_HSJ2);
    pub const IMAGE_IEF: Encoding = Encoding::new(prefix::IMAGE_IEF);
    pub const IMAGE_J2C: Encoding = Encoding::new(prefix::IMAGE_J2C);
    pub const IMAGE_JLS: Encoding = Encoding::new(prefix::IMAGE_JLS);
    pub const IMAGE_JP2: Encoding = Encoding::new(prefix::IMAGE_JP2);
    pub const IMAGE_JPEG: Encoding = Encoding::new(prefix::IMAGE_JPEG);
    pub const IMAGE_JPH: Encoding = Encoding::new(prefix::IMAGE_JPH);
    pub const IMAGE_JPHC: Encoding = Encoding::new(prefix::IMAGE_JPHC);
    pub const IMAGE_JPM: Encoding = Encoding::new(prefix::IMAGE_JPM);
    pub const IMAGE_JPX: Encoding = Encoding::new(prefix::IMAGE_JPX);
    pub const IMAGE_JXR: Encoding = Encoding::new(prefix::IMAGE_JXR);
    pub const IMAGE_JXRA: Encoding = Encoding::new(prefix::IMAGE_JXRA);
    pub const IMAGE_JXRS: Encoding = Encoding::new(prefix::IMAGE_JXRS);
    pub const IMAGE_JXS: Encoding = Encoding::new(prefix::IMAGE_JXS);
    pub const IMAGE_JXSC: Encoding = Encoding::new(prefix::IMAGE_JXSC);
    pub const IMAGE_JXSI: Encoding = Encoding::new(prefix::IMAGE_JXSI);
    pub const IMAGE_JXSS: Encoding = Encoding::new(prefix::IMAGE_JXSS);
    pub const IMAGE_KTX: Encoding = Encoding::new(prefix::IMAGE_KTX);
    pub const IMAGE_KTX2: Encoding = Encoding::new(prefix::IMAGE_KTX2);
    pub const IMAGE_NAPLPS: Encoding = Encoding::new(prefix::IMAGE_NAPLPS);
    pub const IMAGE_PNG: Encoding = Encoding::new(prefix::IMAGE_PNG);
    pub const IMAGE_PRS_BTIF: Encoding = Encoding::new(prefix::IMAGE_PRS_BTIF);
    pub const IMAGE_PRS_PTI: Encoding = Encoding::new(prefix::IMAGE_PRS_PTI);
    pub const IMAGE_PWG_RASTER: Encoding = Encoding::new(prefix::IMAGE_PWG_RASTER);
    pub const IMAGE_SVG_XML: Encoding = Encoding::new(prefix::IMAGE_SVG_XML);
    pub const IMAGE_T38: Encoding = Encoding::new(prefix::IMAGE_T38);
    pub const IMAGE_TIFF: Encoding = Encoding::new(prefix::IMAGE_TIFF);
    pub const IMAGE_TIFF_FX: Encoding = Encoding::new(prefix::IMAGE_TIFF_FX);
    pub const IMAGE_VND_ADOBE_PHOTOSHOP: Encoding =
        Encoding::new(prefix::IMAGE_VND_ADOBE_PHOTOSHOP);
    pub const IMAGE_VND_AIRZIP_ACCELERATOR_AZV: Encoding =
        Encoding::new(prefix::IMAGE_VND_AIRZIP_ACCELERATOR_AZV);
    pub const IMAGE_VND_CNS_INF2: Encoding = Encoding::new(prefix::IMAGE_VND_CNS_INF2);
    pub const IMAGE_VND_DECE_GRAPHIC: Encoding = Encoding::new(prefix::IMAGE_VND_DECE_GRAPHIC);
    pub const IMAGE_VND_DJVU: Encoding = Encoding::new(prefix::IMAGE_VND_DJVU);
    pub const IMAGE_VND_DVB_SUBTITLE: Encoding = Encoding::new(prefix::IMAGE_VND_DVB_SUBTITLE);
    pub const IMAGE_VND_DWG: Encoding = Encoding::new(prefix::IMAGE_VND_DWG);
    pub const IMAGE_VND_DXF: Encoding = Encoding::new(prefix::IMAGE_VND_DXF);
    pub const IMAGE_VND_FASTBIDSHEET: Encoding = Encoding::new(prefix::IMAGE_VND_FASTBIDSHEET);
    pub const IMAGE_VND_FPX: Encoding = Encoding::new(prefix::IMAGE_VND_FPX);
    pub const IMAGE_VND_FST: Encoding = Encoding::new(prefix::IMAGE_VND_FST);
    pub const IMAGE_VND_FUJIXEROX_EDMICS_MMR: Encoding =
        Encoding::new(prefix::IMAGE_VND_FUJIXEROX_EDMICS_MMR);
    pub const IMAGE_VND_FUJIXEROX_EDMICS_RLC: Encoding =
        Encoding::new(prefix::IMAGE_VND_FUJIXEROX_EDMICS_RLC);
    pub const IMAGE_VND_GLOBALGRAPHICS_PGB: Encoding =
        Encoding::new(prefix::IMAGE_VND_GLOBALGRAPHICS_PGB);
    pub const IMAGE_VND_MICROSOFT_ICON: Encoding = Encoding::new(prefix::IMAGE_VND_MICROSOFT_ICON);
    pub const IMAGE_VND_MIX: Encoding = Encoding::new(prefix::IMAGE_VND_MIX);
    pub const IMAGE_VND_MOZILLA_APNG: Encoding = Encoding::new(prefix::IMAGE_VND_MOZILLA_APNG);
    pub const IMAGE_VND_MS_MODI: Encoding = Encoding::new(prefix::IMAGE_VND_MS_MODI);
    pub const IMAGE_VND_NET_FPX: Encoding = Encoding::new(prefix::IMAGE_VND_NET_FPX);
    pub const IMAGE_VND_PCO_B16: Encoding = Encoding::new(prefix::IMAGE_VND_PCO_B16);
    pub const IMAGE_VND_RADIANCE: Encoding = Encoding::new(prefix::IMAGE_VND_RADIANCE);
    pub const IMAGE_VND_SEALED_PNG: Encoding = Encoding::new(prefix::IMAGE_VND_SEALED_PNG);
    pub const IMAGE_VND_SEALEDMEDIA_SOFTSEAL_GIF: Encoding =
        Encoding::new(prefix::IMAGE_VND_SEALEDMEDIA_SOFTSEAL_GIF);
    pub const IMAGE_VND_SEALEDMEDIA_SOFTSEAL_JPG: Encoding =
        Encoding::new(prefix::IMAGE_VND_SEALEDMEDIA_SOFTSEAL_JPG);
    pub const IMAGE_VND_SVF: Encoding = Encoding::new(prefix::IMAGE_VND_SVF);
    pub const IMAGE_VND_TENCENT_TAP: Encoding = Encoding::new(prefix::IMAGE_VND_TENCENT_TAP);
    pub const IMAGE_VND_VALVE_SOURCE_TEXTURE: Encoding =
        Encoding::new(prefix::IMAGE_VND_VALVE_SOURCE_TEXTURE);
    pub const IMAGE_VND_WAP_WBMP: Encoding = Encoding::new(prefix::IMAGE_VND_WAP_WBMP);
    pub const IMAGE_VND_XIFF: Encoding = Encoding::new(prefix::IMAGE_VND_XIFF);
    pub const IMAGE_VND_ZBRUSH_PCX: Encoding = Encoding::new(prefix::IMAGE_VND_ZBRUSH_PCX);
    pub const IMAGE_WEBP: Encoding = Encoding::new(prefix::IMAGE_WEBP);
    pub const IMAGE_WMF: Encoding = Encoding::new(prefix::IMAGE_WMF);
    pub const MESSAGE_CPIM: Encoding = Encoding::new(prefix::MESSAGE_CPIM);
    pub const MESSAGE_BHTTP: Encoding = Encoding::new(prefix::MESSAGE_BHTTP);
    pub const MESSAGE_DELIVERY_STATUS: Encoding = Encoding::new(prefix::MESSAGE_DELIVERY_STATUS);
    pub const MESSAGE_DISPOSITION_NOTIFICATION: Encoding =
        Encoding::new(prefix::MESSAGE_DISPOSITION_NOTIFICATION);
    pub const MESSAGE_EXAMPLE: Encoding = Encoding::new(prefix::MESSAGE_EXAMPLE);
    pub const MESSAGE_EXTERNAL_BODY: Encoding = Encoding::new(prefix::MESSAGE_EXTERNAL_BODY);
    pub const MESSAGE_FEEDBACK_REPORT: Encoding = Encoding::new(prefix::MESSAGE_FEEDBACK_REPORT);
    pub const MESSAGE_GLOBAL: Encoding = Encoding::new(prefix::MESSAGE_GLOBAL);
    pub const MESSAGE_GLOBAL_DELIVERY_STATUS: Encoding =
        Encoding::new(prefix::MESSAGE_GLOBAL_DELIVERY_STATUS);
    pub const MESSAGE_GLOBAL_DISPOSITION_NOTIFICATION: Encoding =
        Encoding::new(prefix::MESSAGE_GLOBAL_DISPOSITION_NOTIFICATION);
    pub const MESSAGE_GLOBAL_HEADERS: Encoding = Encoding::new(prefix::MESSAGE_GLOBAL_HEADERS);
    pub const MESSAGE_HTTP: Encoding = Encoding::new(prefix::MESSAGE_HTTP);
    pub const MESSAGE_IMDN_XML: Encoding = Encoding::new(prefix::MESSAGE_IMDN_XML);
    pub const MESSAGE_MLS: Encoding = Encoding::new(prefix::MESSAGE_MLS);
    pub const MESSAGE_NEWS: Encoding = Encoding::new(prefix::MESSAGE_NEWS);
    pub const MESSAGE_OHTTP_REQ: Encoding = Encoding::new(prefix::MESSAGE_OHTTP_REQ);
    pub const MESSAGE_OHTTP_RES: Encoding = Encoding::new(prefix::MESSAGE_OHTTP_RES);
    pub const MESSAGE_PARTIAL: Encoding = Encoding::new(prefix::MESSAGE_PARTIAL);
    pub const MESSAGE_RFC822: Encoding = Encoding::new(prefix::MESSAGE_RFC822);
    pub const MESSAGE_S_HTTP: Encoding = Encoding::new(prefix::MESSAGE_S_HTTP);
    pub const MESSAGE_SIP: Encoding = Encoding::new(prefix::MESSAGE_SIP);
    pub const MESSAGE_SIPFRAG: Encoding = Encoding::new(prefix::MESSAGE_SIPFRAG);
    pub const MESSAGE_TRACKING_STATUS: Encoding = Encoding::new(prefix::MESSAGE_TRACKING_STATUS);
    pub const MESSAGE_VND_SI_SIMP: Encoding = Encoding::new(prefix::MESSAGE_VND_SI_SIMP);
    pub const MESSAGE_VND_WFA_WSC: Encoding = Encoding::new(prefix::MESSAGE_VND_WFA_WSC);
    pub const MODEL_3MF: Encoding = Encoding::new(prefix::MODEL_3MF);
    pub const MODEL_JT: Encoding = Encoding::new(prefix::MODEL_JT);
    pub const MODEL_E57: Encoding = Encoding::new(prefix::MODEL_E57);
    pub const MODEL_EXAMPLE: Encoding = Encoding::new(prefix::MODEL_EXAMPLE);
    pub const MODEL_GLTF_JSON: Encoding = Encoding::new(prefix::MODEL_GLTF_JSON);
    pub const MODEL_GLTF_BINARY: Encoding = Encoding::new(prefix::MODEL_GLTF_BINARY);
    pub const MODEL_IGES: Encoding = Encoding::new(prefix::MODEL_IGES);
    pub const MODEL_MESH: Encoding = Encoding::new(prefix::MODEL_MESH);
    pub const MODEL_MTL: Encoding = Encoding::new(prefix::MODEL_MTL);
    pub const MODEL_OBJ: Encoding = Encoding::new(prefix::MODEL_OBJ);
    pub const MODEL_PRC: Encoding = Encoding::new(prefix::MODEL_PRC);
    pub const MODEL_STEP: Encoding = Encoding::new(prefix::MODEL_STEP);
    pub const MODEL_STEP_XML: Encoding = Encoding::new(prefix::MODEL_STEP_XML);
    pub const MODEL_STEP_ZIP: Encoding = Encoding::new(prefix::MODEL_STEP_ZIP);
    pub const MODEL_STEP_XML_ZIP: Encoding = Encoding::new(prefix::MODEL_STEP_XML_ZIP);
    pub const MODEL_STL: Encoding = Encoding::new(prefix::MODEL_STL);
    pub const MODEL_U3D: Encoding = Encoding::new(prefix::MODEL_U3D);
    pub const MODEL_VND_BARY: Encoding = Encoding::new(prefix::MODEL_VND_BARY);
    pub const MODEL_VND_CLD: Encoding = Encoding::new(prefix::MODEL_VND_CLD);
    pub const MODEL_VND_COLLADA_XML: Encoding = Encoding::new(prefix::MODEL_VND_COLLADA_XML);
    pub const MODEL_VND_DWF: Encoding = Encoding::new(prefix::MODEL_VND_DWF);
    pub const MODEL_VND_FLATLAND_3DML: Encoding = Encoding::new(prefix::MODEL_VND_FLATLAND_3DML);
    pub const MODEL_VND_GDL: Encoding = Encoding::new(prefix::MODEL_VND_GDL);
    pub const MODEL_VND_GS_GDL: Encoding = Encoding::new(prefix::MODEL_VND_GS_GDL);
    pub const MODEL_VND_GTW: Encoding = Encoding::new(prefix::MODEL_VND_GTW);
    pub const MODEL_VND_MOML_XML: Encoding = Encoding::new(prefix::MODEL_VND_MOML_XML);
    pub const MODEL_VND_MTS: Encoding = Encoding::new(prefix::MODEL_VND_MTS);
    pub const MODEL_VND_OPENGEX: Encoding = Encoding::new(prefix::MODEL_VND_OPENGEX);
    pub const MODEL_VND_PARASOLID_TRANSMIT_BINARY: Encoding =
        Encoding::new(prefix::MODEL_VND_PARASOLID_TRANSMIT_BINARY);
    pub const MODEL_VND_PARASOLID_TRANSMIT_TEXT: Encoding =
        Encoding::new(prefix::MODEL_VND_PARASOLID_TRANSMIT_TEXT);
    pub const MODEL_VND_PYTHA_PYOX: Encoding = Encoding::new(prefix::MODEL_VND_PYTHA_PYOX);
    pub const MODEL_VND_ROSETTE_ANNOTATED_DATA_MODEL: Encoding =
        Encoding::new(prefix::MODEL_VND_ROSETTE_ANNOTATED_DATA_MODEL);
    pub const MODEL_VND_SAP_VDS: Encoding = Encoding::new(prefix::MODEL_VND_SAP_VDS);
    pub const MODEL_VND_USDA: Encoding = Encoding::new(prefix::MODEL_VND_USDA);
    pub const MODEL_VND_USDZ_ZIP: Encoding = Encoding::new(prefix::MODEL_VND_USDZ_ZIP);
    pub const MODEL_VND_VALVE_SOURCE_COMPILED_MAP: Encoding =
        Encoding::new(prefix::MODEL_VND_VALVE_SOURCE_COMPILED_MAP);
    pub const MODEL_VND_VTU: Encoding = Encoding::new(prefix::MODEL_VND_VTU);
    pub const MODEL_VRML: Encoding = Encoding::new(prefix::MODEL_VRML);
    pub const MODEL_X3D_FASTINFOSET: Encoding = Encoding::new(prefix::MODEL_X3D_FASTINFOSET);
    pub const MODEL_X3D_XML: Encoding = Encoding::new(prefix::MODEL_X3D_XML);
    pub const MODEL_X3D_VRML: Encoding = Encoding::new(prefix::MODEL_X3D_VRML);
    pub const MULTIPART_ALTERNATIVE: Encoding = Encoding::new(prefix::MULTIPART_ALTERNATIVE);
    pub const MULTIPART_APPLEDOUBLE: Encoding = Encoding::new(prefix::MULTIPART_APPLEDOUBLE);
    pub const MULTIPART_BYTERANGES: Encoding = Encoding::new(prefix::MULTIPART_BYTERANGES);
    pub const MULTIPART_DIGEST: Encoding = Encoding::new(prefix::MULTIPART_DIGEST);
    pub const MULTIPART_ENCRYPTED: Encoding = Encoding::new(prefix::MULTIPART_ENCRYPTED);
    pub const MULTIPART_EXAMPLE: Encoding = Encoding::new(prefix::MULTIPART_EXAMPLE);
    pub const MULTIPART_FORM_DATA: Encoding = Encoding::new(prefix::MULTIPART_FORM_DATA);
    pub const MULTIPART_HEADER_SET: Encoding = Encoding::new(prefix::MULTIPART_HEADER_SET);
    pub const MULTIPART_MIXED: Encoding = Encoding::new(prefix::MULTIPART_MIXED);
    pub const MULTIPART_MULTILINGUAL: Encoding = Encoding::new(prefix::MULTIPART_MULTILINGUAL);
    pub const MULTIPART_PARALLEL: Encoding = Encoding::new(prefix::MULTIPART_PARALLEL);
    pub const MULTIPART_RELATED: Encoding = Encoding::new(prefix::MULTIPART_RELATED);
    pub const MULTIPART_REPORT: Encoding = Encoding::new(prefix::MULTIPART_REPORT);
    pub const MULTIPART_SIGNED: Encoding = Encoding::new(prefix::MULTIPART_SIGNED);
    pub const MULTIPART_VND_BINT_MED_PLUS: Encoding =
        Encoding::new(prefix::MULTIPART_VND_BINT_MED_PLUS);
    pub const MULTIPART_VOICE_MESSAGE: Encoding = Encoding::new(prefix::MULTIPART_VOICE_MESSAGE);
    pub const MULTIPART_X_MIXED_REPLACE: Encoding =
        Encoding::new(prefix::MULTIPART_X_MIXED_REPLACE);
    pub const TEXT_1D_INTERLEAVED_PARITYFEC: Encoding =
        Encoding::new(prefix::TEXT_1D_INTERLEAVED_PARITYFEC);
    pub const TEXT_RED: Encoding = Encoding::new(prefix::TEXT_RED);
    pub const TEXT_SGML: Encoding = Encoding::new(prefix::TEXT_SGML);
    pub const TEXT_CACHE_MANIFEST: Encoding = Encoding::new(prefix::TEXT_CACHE_MANIFEST);
    pub const TEXT_CALENDAR: Encoding = Encoding::new(prefix::TEXT_CALENDAR);
    pub const TEXT_CQL: Encoding = Encoding::new(prefix::TEXT_CQL);
    pub const TEXT_CQL_EXPRESSION: Encoding = Encoding::new(prefix::TEXT_CQL_EXPRESSION);
    pub const TEXT_CQL_IDENTIFIER: Encoding = Encoding::new(prefix::TEXT_CQL_IDENTIFIER);
    pub const TEXT_CSS: Encoding = Encoding::new(prefix::TEXT_CSS);
    pub const TEXT_CSV: Encoding = Encoding::new(prefix::TEXT_CSV);
    pub const TEXT_CSV_SCHEMA: Encoding = Encoding::new(prefix::TEXT_CSV_SCHEMA);
    pub const TEXT_DIRECTORY: Encoding = Encoding::new(prefix::TEXT_DIRECTORY);
    pub const TEXT_DNS: Encoding = Encoding::new(prefix::TEXT_DNS);
    pub const TEXT_ECMASCRIPT: Encoding = Encoding::new(prefix::TEXT_ECMASCRIPT);
    pub const TEXT_ENCAPRTP: Encoding = Encoding::new(prefix::TEXT_ENCAPRTP);
    pub const TEXT_ENRICHED: Encoding = Encoding::new(prefix::TEXT_ENRICHED);
    pub const TEXT_EXAMPLE: Encoding = Encoding::new(prefix::TEXT_EXAMPLE);
    pub const TEXT_FHIRPATH: Encoding = Encoding::new(prefix::TEXT_FHIRPATH);
    pub const TEXT_FLEXFEC: Encoding = Encoding::new(prefix::TEXT_FLEXFEC);
    pub const TEXT_FWDRED: Encoding = Encoding::new(prefix::TEXT_FWDRED);
    pub const TEXT_GFF3: Encoding = Encoding::new(prefix::TEXT_GFF3);
    pub const TEXT_GRAMMAR_REF_LIST: Encoding = Encoding::new(prefix::TEXT_GRAMMAR_REF_LIST);
    pub const TEXT_HL7V2: Encoding = Encoding::new(prefix::TEXT_HL7V2);
    pub const TEXT_HTML: Encoding = Encoding::new(prefix::TEXT_HTML);
    pub const TEXT_JAVASCRIPT: Encoding = Encoding::new(prefix::TEXT_JAVASCRIPT);
    pub const TEXT_JCR_CND: Encoding = Encoding::new(prefix::TEXT_JCR_CND);
    pub const TEXT_MARKDOWN: Encoding = Encoding::new(prefix::TEXT_MARKDOWN);
    pub const TEXT_MIZAR: Encoding = Encoding::new(prefix::TEXT_MIZAR);
    pub const TEXT_N3: Encoding = Encoding::new(prefix::TEXT_N3);
    pub const TEXT_PARAMETERS: Encoding = Encoding::new(prefix::TEXT_PARAMETERS);
    pub const TEXT_PARITYFEC: Encoding = Encoding::new(prefix::TEXT_PARITYFEC);
    pub const TEXT_PLAIN: Encoding = Encoding::new(prefix::TEXT_PLAIN);
    pub const TEXT_PROVENANCE_NOTATION: Encoding = Encoding::new(prefix::TEXT_PROVENANCE_NOTATION);
    pub const TEXT_PRS_FALLENSTEIN_RST: Encoding = Encoding::new(prefix::TEXT_PRS_FALLENSTEIN_RST);
    pub const TEXT_PRS_LINES_TAG: Encoding = Encoding::new(prefix::TEXT_PRS_LINES_TAG);
    pub const TEXT_PRS_PROP_LOGIC: Encoding = Encoding::new(prefix::TEXT_PRS_PROP_LOGIC);
    pub const TEXT_PRS_TEXI: Encoding = Encoding::new(prefix::TEXT_PRS_TEXI);
    pub const TEXT_RAPTORFEC: Encoding = Encoding::new(prefix::TEXT_RAPTORFEC);
    pub const TEXT_RFC822_HEADERS: Encoding = Encoding::new(prefix::TEXT_RFC822_HEADERS);
    pub const TEXT_RICHTEXT: Encoding = Encoding::new(prefix::TEXT_RICHTEXT);
    pub const TEXT_RTF: Encoding = Encoding::new(prefix::TEXT_RTF);
    pub const TEXT_RTP_ENC_AESCM128: Encoding = Encoding::new(prefix::TEXT_RTP_ENC_AESCM128);
    pub const TEXT_RTPLOOPBACK: Encoding = Encoding::new(prefix::TEXT_RTPLOOPBACK);
    pub const TEXT_RTX: Encoding = Encoding::new(prefix::TEXT_RTX);
    pub const TEXT_SHACLC: Encoding = Encoding::new(prefix::TEXT_SHACLC);
    pub const TEXT_SHEX: Encoding = Encoding::new(prefix::TEXT_SHEX);
    pub const TEXT_SPDX: Encoding = Encoding::new(prefix::TEXT_SPDX);
    pub const TEXT_STRINGS: Encoding = Encoding::new(prefix::TEXT_STRINGS);
    pub const TEXT_T140: Encoding = Encoding::new(prefix::TEXT_T140);
    pub const TEXT_TAB_SEPARATED_VALUES: Encoding =
        Encoding::new(prefix::TEXT_TAB_SEPARATED_VALUES);
    pub const TEXT_TROFF: Encoding = Encoding::new(prefix::TEXT_TROFF);
    pub const TEXT_TURTLE: Encoding = Encoding::new(prefix::TEXT_TURTLE);
    pub const TEXT_ULPFEC: Encoding = Encoding::new(prefix::TEXT_ULPFEC);
    pub const TEXT_URI_LIST: Encoding = Encoding::new(prefix::TEXT_URI_LIST);
    pub const TEXT_VCARD: Encoding = Encoding::new(prefix::TEXT_VCARD);
    pub const TEXT_VND_DMCLIENTSCRIPT: Encoding = Encoding::new(prefix::TEXT_VND_DMCLIENTSCRIPT);
    pub const TEXT_VND_IPTC_NITF: Encoding = Encoding::new(prefix::TEXT_VND_IPTC_NITF);
    pub const TEXT_VND_IPTC_NEWSML: Encoding = Encoding::new(prefix::TEXT_VND_IPTC_NEWSML);
    pub const TEXT_VND_A: Encoding = Encoding::new(prefix::TEXT_VND_A);
    pub const TEXT_VND_ABC: Encoding = Encoding::new(prefix::TEXT_VND_ABC);
    pub const TEXT_VND_ASCII_ART: Encoding = Encoding::new(prefix::TEXT_VND_ASCII_ART);
    pub const TEXT_VND_CURL: Encoding = Encoding::new(prefix::TEXT_VND_CURL);
    pub const TEXT_VND_DEBIAN_COPYRIGHT: Encoding =
        Encoding::new(prefix::TEXT_VND_DEBIAN_COPYRIGHT);
    pub const TEXT_VND_DVB_SUBTITLE: Encoding = Encoding::new(prefix::TEXT_VND_DVB_SUBTITLE);
    pub const TEXT_VND_ESMERTEC_THEME_DESCRIPTOR: Encoding =
        Encoding::new(prefix::TEXT_VND_ESMERTEC_THEME_DESCRIPTOR);
    pub const TEXT_VND_EXCHANGEABLE: Encoding = Encoding::new(prefix::TEXT_VND_EXCHANGEABLE);
    pub const TEXT_VND_FAMILYSEARCH_GEDCOM: Encoding =
        Encoding::new(prefix::TEXT_VND_FAMILYSEARCH_GEDCOM);
    pub const TEXT_VND_FICLAB_FLT: Encoding = Encoding::new(prefix::TEXT_VND_FICLAB_FLT);
    pub const TEXT_VND_FLY: Encoding = Encoding::new(prefix::TEXT_VND_FLY);
    pub const TEXT_VND_FMI_FLEXSTOR: Encoding = Encoding::new(prefix::TEXT_VND_FMI_FLEXSTOR);
    pub const TEXT_VND_GML: Encoding = Encoding::new(prefix::TEXT_VND_GML);
    pub const TEXT_VND_GRAPHVIZ: Encoding = Encoding::new(prefix::TEXT_VND_GRAPHVIZ);
    pub const TEXT_VND_HANS: Encoding = Encoding::new(prefix::TEXT_VND_HANS);
    pub const TEXT_VND_HGL: Encoding = Encoding::new(prefix::TEXT_VND_HGL);
    pub const TEXT_VND_IN3D_3DML: Encoding = Encoding::new(prefix::TEXT_VND_IN3D_3DML);
    pub const TEXT_VND_IN3D_SPOT: Encoding = Encoding::new(prefix::TEXT_VND_IN3D_SPOT);
    pub const TEXT_VND_LATEX_Z: Encoding = Encoding::new(prefix::TEXT_VND_LATEX_Z);
    pub const TEXT_VND_MOTOROLA_REFLEX: Encoding = Encoding::new(prefix::TEXT_VND_MOTOROLA_REFLEX);
    pub const TEXT_VND_MS_MEDIAPACKAGE: Encoding = Encoding::new(prefix::TEXT_VND_MS_MEDIAPACKAGE);
    pub const TEXT_VND_NET2PHONE_COMMCENTER_COMMAND: Encoding =
        Encoding::new(prefix::TEXT_VND_NET2PHONE_COMMCENTER_COMMAND);
    pub const TEXT_VND_RADISYS_MSML_BASIC_LAYOUT: Encoding =
        Encoding::new(prefix::TEXT_VND_RADISYS_MSML_BASIC_LAYOUT);
    pub const TEXT_VND_SENX_WARPSCRIPT: Encoding = Encoding::new(prefix::TEXT_VND_SENX_WARPSCRIPT);
    pub const TEXT_VND_SI_URICATALOGUE: Encoding = Encoding::new(prefix::TEXT_VND_SI_URICATALOGUE);
    pub const TEXT_VND_SOSI: Encoding = Encoding::new(prefix::TEXT_VND_SOSI);
    pub const TEXT_VND_SUN_J2ME_APP_DESCRIPTOR: Encoding =
        Encoding::new(prefix::TEXT_VND_SUN_J2ME_APP_DESCRIPTOR);
    pub const TEXT_VND_TROLLTECH_LINGUIST: Encoding =
        Encoding::new(prefix::TEXT_VND_TROLLTECH_LINGUIST);
    pub const TEXT_VND_WAP_SI: Encoding = Encoding::new(prefix::TEXT_VND_WAP_SI);
    pub const TEXT_VND_WAP_SL: Encoding = Encoding::new(prefix::TEXT_VND_WAP_SL);
    pub const TEXT_VND_WAP_WML: Encoding = Encoding::new(prefix::TEXT_VND_WAP_WML);
    pub const TEXT_VND_WAP_WMLSCRIPT: Encoding = Encoding::new(prefix::TEXT_VND_WAP_WMLSCRIPT);
    pub const TEXT_VTT: Encoding = Encoding::new(prefix::TEXT_VTT);
    pub const TEXT_WGSL: Encoding = Encoding::new(prefix::TEXT_WGSL);
    pub const TEXT_XML: Encoding = Encoding::new(prefix::TEXT_XML);
    pub const TEXT_XML_EXTERNAL_PARSED_ENTITY: Encoding =
        Encoding::new(prefix::TEXT_XML_EXTERNAL_PARSED_ENTITY);
    pub const VIDEO_1D_INTERLEAVED_PARITYFEC: Encoding =
        Encoding::new(prefix::VIDEO_1D_INTERLEAVED_PARITYFEC);
    pub const VIDEO_3GPP: Encoding = Encoding::new(prefix::VIDEO_3GPP);
    pub const VIDEO_3GPP_TT: Encoding = Encoding::new(prefix::VIDEO_3GPP_TT);
    pub const VIDEO_3GPP2: Encoding = Encoding::new(prefix::VIDEO_3GPP2);
    pub const VIDEO_AV1: Encoding = Encoding::new(prefix::VIDEO_AV1);
    pub const VIDEO_BMPEG: Encoding = Encoding::new(prefix::VIDEO_BMPEG);
    pub const VIDEO_BT656: Encoding = Encoding::new(prefix::VIDEO_BT656);
    pub const VIDEO_CELB: Encoding = Encoding::new(prefix::VIDEO_CELB);
    pub const VIDEO_DV: Encoding = Encoding::new(prefix::VIDEO_DV);
    pub const VIDEO_FFV1: Encoding = Encoding::new(prefix::VIDEO_FFV1);
    pub const VIDEO_H261: Encoding = Encoding::new(prefix::VIDEO_H261);
    pub const VIDEO_H263: Encoding = Encoding::new(prefix::VIDEO_H263);
    pub const VIDEO_H263_1998: Encoding = Encoding::new(prefix::VIDEO_H263_1998);
    pub const VIDEO_H263_2000: Encoding = Encoding::new(prefix::VIDEO_H263_2000);
    pub const VIDEO_H264: Encoding = Encoding::new(prefix::VIDEO_H264);
    pub const VIDEO_H264_RCDO: Encoding = Encoding::new(prefix::VIDEO_H264_RCDO);
    pub const VIDEO_H264_SVC: Encoding = Encoding::new(prefix::VIDEO_H264_SVC);
    pub const VIDEO_H265: Encoding = Encoding::new(prefix::VIDEO_H265);
    pub const VIDEO_H266: Encoding = Encoding::new(prefix::VIDEO_H266);
    pub const VIDEO_JPEG: Encoding = Encoding::new(prefix::VIDEO_JPEG);
    pub const VIDEO_MP1S: Encoding = Encoding::new(prefix::VIDEO_MP1S);
    pub const VIDEO_MP2P: Encoding = Encoding::new(prefix::VIDEO_MP2P);
    pub const VIDEO_MP2T: Encoding = Encoding::new(prefix::VIDEO_MP2T);
    pub const VIDEO_MP4V_ES: Encoding = Encoding::new(prefix::VIDEO_MP4V_ES);
    pub const VIDEO_MPV: Encoding = Encoding::new(prefix::VIDEO_MPV);
    pub const VIDEO_SMPTE292M: Encoding = Encoding::new(prefix::VIDEO_SMPTE292M);
    pub const VIDEO_VP8: Encoding = Encoding::new(prefix::VIDEO_VP8);
    pub const VIDEO_VP9: Encoding = Encoding::new(prefix::VIDEO_VP9);
    pub const VIDEO_ENCAPRTP: Encoding = Encoding::new(prefix::VIDEO_ENCAPRTP);
    pub const VIDEO_EVC: Encoding = Encoding::new(prefix::VIDEO_EVC);
    pub const VIDEO_EXAMPLE: Encoding = Encoding::new(prefix::VIDEO_EXAMPLE);
    pub const VIDEO_FLEXFEC: Encoding = Encoding::new(prefix::VIDEO_FLEXFEC);
    pub const VIDEO_ISO_SEGMENT: Encoding = Encoding::new(prefix::VIDEO_ISO_SEGMENT);
    pub const VIDEO_JPEG2000: Encoding = Encoding::new(prefix::VIDEO_JPEG2000);
    pub const VIDEO_JXSV: Encoding = Encoding::new(prefix::VIDEO_JXSV);
    pub const VIDEO_MATROSKA: Encoding = Encoding::new(prefix::VIDEO_MATROSKA);
    pub const VIDEO_MATROSKA_3D: Encoding = Encoding::new(prefix::VIDEO_MATROSKA_3D);
    pub const VIDEO_MJ2: Encoding = Encoding::new(prefix::VIDEO_MJ2);
    pub const VIDEO_MP4: Encoding = Encoding::new(prefix::VIDEO_MP4);
    pub const VIDEO_MPEG: Encoding = Encoding::new(prefix::VIDEO_MPEG);
    pub const VIDEO_MPEG4_GENERIC: Encoding = Encoding::new(prefix::VIDEO_MPEG4_GENERIC);
    pub const VIDEO_NV: Encoding = Encoding::new(prefix::VIDEO_NV);
    pub const VIDEO_OGG: Encoding = Encoding::new(prefix::VIDEO_OGG);
    pub const VIDEO_PARITYFEC: Encoding = Encoding::new(prefix::VIDEO_PARITYFEC);
    pub const VIDEO_POINTER: Encoding = Encoding::new(prefix::VIDEO_POINTER);
    pub const VIDEO_QUICKTIME: Encoding = Encoding::new(prefix::VIDEO_QUICKTIME);
    pub const VIDEO_RAPTORFEC: Encoding = Encoding::new(prefix::VIDEO_RAPTORFEC);
    pub const VIDEO_RAW: Encoding = Encoding::new(prefix::VIDEO_RAW);
    pub const VIDEO_RTP_ENC_AESCM128: Encoding = Encoding::new(prefix::VIDEO_RTP_ENC_AESCM128);
    pub const VIDEO_RTPLOOPBACK: Encoding = Encoding::new(prefix::VIDEO_RTPLOOPBACK);
    pub const VIDEO_RTX: Encoding = Encoding::new(prefix::VIDEO_RTX);
    pub const VIDEO_SCIP: Encoding = Encoding::new(prefix::VIDEO_SCIP);
    pub const VIDEO_SMPTE291: Encoding = Encoding::new(prefix::VIDEO_SMPTE291);
    pub const VIDEO_ULPFEC: Encoding = Encoding::new(prefix::VIDEO_ULPFEC);
    pub const VIDEO_VC1: Encoding = Encoding::new(prefix::VIDEO_VC1);
    pub const VIDEO_VC2: Encoding = Encoding::new(prefix::VIDEO_VC2);
    pub const VIDEO_VND_CCTV: Encoding = Encoding::new(prefix::VIDEO_VND_CCTV);
    pub const VIDEO_VND_DECE_HD: Encoding = Encoding::new(prefix::VIDEO_VND_DECE_HD);
    pub const VIDEO_VND_DECE_MOBILE: Encoding = Encoding::new(prefix::VIDEO_VND_DECE_MOBILE);
    pub const VIDEO_VND_DECE_MP4: Encoding = Encoding::new(prefix::VIDEO_VND_DECE_MP4);
    pub const VIDEO_VND_DECE_PD: Encoding = Encoding::new(prefix::VIDEO_VND_DECE_PD);
    pub const VIDEO_VND_DECE_SD: Encoding = Encoding::new(prefix::VIDEO_VND_DECE_SD);
    pub const VIDEO_VND_DECE_VIDEO: Encoding = Encoding::new(prefix::VIDEO_VND_DECE_VIDEO);
    pub const VIDEO_VND_DIRECTV_MPEG: Encoding = Encoding::new(prefix::VIDEO_VND_DIRECTV_MPEG);
    pub const VIDEO_VND_DIRECTV_MPEG_TTS: Encoding =
        Encoding::new(prefix::VIDEO_VND_DIRECTV_MPEG_TTS);
    pub const VIDEO_VND_DLNA_MPEG_TTS: Encoding = Encoding::new(prefix::VIDEO_VND_DLNA_MPEG_TTS);
    pub const VIDEO_VND_DVB_FILE: Encoding = Encoding::new(prefix::VIDEO_VND_DVB_FILE);
    pub const VIDEO_VND_FVT: Encoding = Encoding::new(prefix::VIDEO_VND_FVT);
    pub const VIDEO_VND_HNS_VIDEO: Encoding = Encoding::new(prefix::VIDEO_VND_HNS_VIDEO);
    pub const VIDEO_VND_IPTVFORUM_1DPARITYFEC_1010: Encoding =
        Encoding::new(prefix::VIDEO_VND_IPTVFORUM_1DPARITYFEC_1010);
    pub const VIDEO_VND_IPTVFORUM_1DPARITYFEC_2005: Encoding =
        Encoding::new(prefix::VIDEO_VND_IPTVFORUM_1DPARITYFEC_2005);
    pub const VIDEO_VND_IPTVFORUM_2DPARITYFEC_1010: Encoding =
        Encoding::new(prefix::VIDEO_VND_IPTVFORUM_2DPARITYFEC_1010);
    pub const VIDEO_VND_IPTVFORUM_2DPARITYFEC_2005: Encoding =
        Encoding::new(prefix::VIDEO_VND_IPTVFORUM_2DPARITYFEC_2005);
    pub const VIDEO_VND_IPTVFORUM_TTSAVC: Encoding =
        Encoding::new(prefix::VIDEO_VND_IPTVFORUM_TTSAVC);
    pub const VIDEO_VND_IPTVFORUM_TTSMPEG2: Encoding =
        Encoding::new(prefix::VIDEO_VND_IPTVFORUM_TTSMPEG2);
    pub const VIDEO_VND_MOTOROLA_VIDEO: Encoding = Encoding::new(prefix::VIDEO_VND_MOTOROLA_VIDEO);
    pub const VIDEO_VND_MOTOROLA_VIDEOP: Encoding =
        Encoding::new(prefix::VIDEO_VND_MOTOROLA_VIDEOP);
    pub const VIDEO_VND_MPEGURL: Encoding = Encoding::new(prefix::VIDEO_VND_MPEGURL);
    pub const VIDEO_VND_MS_PLAYREADY_MEDIA_PYV: Encoding =
        Encoding::new(prefix::VIDEO_VND_MS_PLAYREADY_MEDIA_PYV);
    pub const VIDEO_VND_NOKIA_INTERLEAVED_MULTIMEDIA: Encoding =
        Encoding::new(prefix::VIDEO_VND_NOKIA_INTERLEAVED_MULTIMEDIA);
    pub const VIDEO_VND_NOKIA_MP4VR: Encoding = Encoding::new(prefix::VIDEO_VND_NOKIA_MP4VR);
    pub const VIDEO_VND_NOKIA_VIDEOVOIP: Encoding =
        Encoding::new(prefix::VIDEO_VND_NOKIA_VIDEOVOIP);
    pub const VIDEO_VND_OBJECTVIDEO: Encoding = Encoding::new(prefix::VIDEO_VND_OBJECTVIDEO);
    pub const VIDEO_VND_RADGAMETTOOLS_BINK: Encoding =
        Encoding::new(prefix::VIDEO_VND_RADGAMETTOOLS_BINK);
    pub const VIDEO_VND_RADGAMETTOOLS_SMACKER: Encoding =
        Encoding::new(prefix::VIDEO_VND_RADGAMETTOOLS_SMACKER);
    pub const VIDEO_VND_SEALED_MPEG1: Encoding = Encoding::new(prefix::VIDEO_VND_SEALED_MPEG1);
    pub const VIDEO_VND_SEALED_MPEG4: Encoding = Encoding::new(prefix::VIDEO_VND_SEALED_MPEG4);
    pub const VIDEO_VND_SEALED_SWF: Encoding = Encoding::new(prefix::VIDEO_VND_SEALED_SWF);
    pub const VIDEO_VND_SEALEDMEDIA_SOFTSEAL_MOV: Encoding =
        Encoding::new(prefix::VIDEO_VND_SEALEDMEDIA_SOFTSEAL_MOV);
    pub const VIDEO_VND_UVVU_MP4: Encoding = Encoding::new(prefix::VIDEO_VND_UVVU_MP4);
    pub const VIDEO_VND_VIVO: Encoding = Encoding::new(prefix::VIDEO_VND_VIVO);
    pub const VIDEO_VND_YOUTUBE_YT: Encoding = Encoding::new(prefix::VIDEO_VND_YOUTUBE_YT);
}

impl EncodingMapping for IanaEncodingMapping {
    const MIN: EncodingPrefix = 1024;
    const MAX: EncodingPrefix = 3120;

    /// Given a numerical [`EncodingPrefix`] returns its string representation.
    fn prefix_to_str(&self, p: EncodingPrefix) -> Option<Cow<'static, str>> {
        prefix::KNOWN_PREFIX.get(&p).map(|s| Cow::Borrowed(*s))
    }

    /// Given the string representation of a prefix returns its numerical representation as [`EncodingPrefix`].
    /// [EMPTY](`prefix::EMPTY`) is returned in case of unknown mapping.
    fn str_to_prefix(&self, s: &str) -> Option<EncodingPrefix> {
        prefix::KNOWN_STRING.get(s).copied()
    }

    /// Parse a string into a valid [`Encoding`]. This functions performs the necessary
    /// prefix mapping and suffix substring when parsing the input. In case of unknown prefix mapping,
    /// the [prefix](`Encoding::prefix`) will be set to [EMPTY](`prefix::EMPTY`) and the
    /// full string will be part of the [suffix](`Encoding::suffix`).
    fn parse<S>(&self, t: S) -> ZResult<Encoding>
    where
        S: Into<Cow<'static, str>>,
    {
        fn _parse(_self: &IanaEncodingMapping, t: Cow<'static, str>) -> ZResult<Encoding> {
            // Check if empty
            if t.is_empty() {
                return Ok(IanaEncoding::EMPTY);
            }
            // Try then an lookup of the string to prefix for the IanaEncodingMapping
            if let Some(p) = _self.str_to_prefix(t.as_ref()) {
                return Ok(Encoding::new(p));
            }
            // Check if the passed string matches one of the known prefixes. It will map the known string
            // prefix to the numerical prefix and carry the remaining part of the string in the suffix.
            // Skip empty string mapping. The order is guaranteed by the phf::OrderedMap.
            for (s, p) in prefix::KNOWN_STRING.entries().skip(1) {
                if let Some(i) = t.find(s) {
                    let e = Encoding::new(*p);
                    match t {
                        Cow::Borrowed(s) => return e.with_suffix(s.split_at(i + s.len()).1),
                        Cow::Owned(mut s) => return e.with_suffix(s.split_off(i + s.len())),
                    }
                }
            }
            IanaEncoding::EMPTY.with_suffix(t)
        }
        _parse(self, t.into())
    }

    /// Given an [`Encoding`] returns a full string representation.
    /// It concatenates the string represenation of the encoding prefix with the encoding suffix.
    fn to_str(&self, e: &Encoding) -> Cow<'_, str> {
        let (p, s) = (e.prefix(), e.suffix());
        match self.prefix_to_str(p) {
            Some(p) if s.is_empty() => p,
            Some(p) => Cow::Owned(format!("{}{}", p, s)),
            None => Cow::Owned(format!("unknown({}){}", p, s)),
        }
    }
}

crate::derive_default_encoding_for!(IanaEncoding);
