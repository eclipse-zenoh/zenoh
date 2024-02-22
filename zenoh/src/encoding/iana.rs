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
use crate::encoding::EncodingMapping;
use phf::phf_ordered_map;
use std::borrow::Cow;
use zenoh_protocol::core::{Encoding, EncodingPrefix};
use zenoh_result::ZResult;

#[derive(Clone, Copy, Debug)]
pub struct IanaEncodingMapping;

impl IanaEncodingMapping {
    pub const EMPTY: EncodingPrefix = 0;
    pub const APPLICATION_1D_INTERLEAVED_PARITYFEC: EncodingPrefix = 1;
    pub const APPLICATION_3GPDASH_QOE_REPORT_XML: EncodingPrefix = 2;
    pub const APPLICATION_3GPP_IMS_XML: EncodingPrefix = 3;
    pub const APPLICATION_3GPPHAL_JSON: EncodingPrefix = 4;
    pub const APPLICATION_3GPPHALFORMS_JSON: EncodingPrefix = 5;
    pub const APPLICATION_A2L: EncodingPrefix = 6;
    pub const APPLICATION_AML: EncodingPrefix = 7;
    pub const APPLICATION_ATF: EncodingPrefix = 8;
    pub const APPLICATION_ATFX: EncodingPrefix = 9;
    pub const APPLICATION_ATXML: EncodingPrefix = 10;
    pub const APPLICATION_CALS_1840: EncodingPrefix = 11;
    pub const APPLICATION_CDFX_XML: EncodingPrefix = 12;
    pub const APPLICATION_CEA: EncodingPrefix = 13;
    pub const APPLICATION_CSTADATA_XML: EncodingPrefix = 14;
    pub const APPLICATION_DCD: EncodingPrefix = 15;
    pub const APPLICATION_DII: EncodingPrefix = 16;
    pub const APPLICATION_DIT: EncodingPrefix = 17;
    pub const APPLICATION_EDI_X12: EncodingPrefix = 18;
    pub const APPLICATION_EDI_CONSENT: EncodingPrefix = 19;
    pub const APPLICATION_EDIFACT: EncodingPrefix = 20;
    pub const APPLICATION_EMERGENCYCALLDATA_COMMENT_XML: EncodingPrefix = 21;
    pub const APPLICATION_EMERGENCYCALLDATA_CONTROL_XML: EncodingPrefix = 22;
    pub const APPLICATION_EMERGENCYCALLDATA_DEVICEINFO_XML: EncodingPrefix = 23;
    pub const APPLICATION_EMERGENCYCALLDATA_LEGACYESN_JSON: EncodingPrefix = 24;
    pub const APPLICATION_EMERGENCYCALLDATA_PROVIDERINFO_XML: EncodingPrefix = 25;
    pub const APPLICATION_EMERGENCYCALLDATA_SERVICEINFO_XML: EncodingPrefix = 26;
    pub const APPLICATION_EMERGENCYCALLDATA_SUBSCRIBERINFO_XML: EncodingPrefix = 27;
    pub const APPLICATION_EMERGENCYCALLDATA_VEDS_XML: EncodingPrefix = 28;
    pub const APPLICATION_EMERGENCYCALLDATA_CAP_XML: EncodingPrefix = 29;
    pub const APPLICATION_EMERGENCYCALLDATA_ECALL_MSD: EncodingPrefix = 30;
    pub const APPLICATION_H224: EncodingPrefix = 31;
    pub const APPLICATION_IOTP: EncodingPrefix = 32;
    pub const APPLICATION_ISUP: EncodingPrefix = 33;
    pub const APPLICATION_LXF: EncodingPrefix = 34;
    pub const APPLICATION_MF4: EncodingPrefix = 35;
    pub const APPLICATION_ODA: EncodingPrefix = 36;
    pub const APPLICATION_ODX: EncodingPrefix = 37;
    pub const APPLICATION_PDX: EncodingPrefix = 38;
    pub const APPLICATION_QSIG: EncodingPrefix = 39;
    pub const APPLICATION_SGML: EncodingPrefix = 40;
    pub const APPLICATION_TETRA_ISI: EncodingPrefix = 41;
    pub const APPLICATION_ACE_CBOR: EncodingPrefix = 42;
    pub const APPLICATION_ACE_JSON: EncodingPrefix = 43;
    pub const APPLICATION_ACTIVEMESSAGE: EncodingPrefix = 44;
    pub const APPLICATION_ACTIVITY_JSON: EncodingPrefix = 45;
    pub const APPLICATION_AIF_CBOR: EncodingPrefix = 46;
    pub const APPLICATION_AIF_JSON: EncodingPrefix = 47;
    pub const APPLICATION_ALTO_CDNI_JSON: EncodingPrefix = 48;
    pub const APPLICATION_ALTO_CDNIFILTER_JSON: EncodingPrefix = 49;
    pub const APPLICATION_ALTO_COSTMAP_JSON: EncodingPrefix = 50;
    pub const APPLICATION_ALTO_COSTMAPFILTER_JSON: EncodingPrefix = 51;
    pub const APPLICATION_ALTO_DIRECTORY_JSON: EncodingPrefix = 52;
    pub const APPLICATION_ALTO_ENDPOINTCOST_JSON: EncodingPrefix = 53;
    pub const APPLICATION_ALTO_ENDPOINTCOSTPARAMS_JSON: EncodingPrefix = 54;
    pub const APPLICATION_ALTO_ENDPOINTPROP_JSON: EncodingPrefix = 55;
    pub const APPLICATION_ALTO_ENDPOINTPROPPARAMS_JSON: EncodingPrefix = 56;
    pub const APPLICATION_ALTO_ERROR_JSON: EncodingPrefix = 57;
    pub const APPLICATION_ALTO_NETWORKMAP_JSON: EncodingPrefix = 58;
    pub const APPLICATION_ALTO_NETWORKMAPFILTER_JSON: EncodingPrefix = 59;
    pub const APPLICATION_ALTO_PROPMAP_JSON: EncodingPrefix = 60;
    pub const APPLICATION_ALTO_PROPMAPPARAMS_JSON: EncodingPrefix = 61;
    pub const APPLICATION_ALTO_TIPS_JSON: EncodingPrefix = 62;
    pub const APPLICATION_ALTO_TIPSPARAMS_JSON: EncodingPrefix = 63;
    pub const APPLICATION_ALTO_UPDATESTREAMCONTROL_JSON: EncodingPrefix = 64;
    pub const APPLICATION_ALTO_UPDATESTREAMPARAMS_JSON: EncodingPrefix = 65;
    pub const APPLICATION_ANDREW_INSET: EncodingPrefix = 66;
    pub const APPLICATION_APPLEFILE: EncodingPrefix = 67;
    pub const APPLICATION_AT_JWT: EncodingPrefix = 68;
    pub const APPLICATION_ATOM_XML: EncodingPrefix = 69;
    pub const APPLICATION_ATOMCAT_XML: EncodingPrefix = 70;
    pub const APPLICATION_ATOMDELETED_XML: EncodingPrefix = 71;
    pub const APPLICATION_ATOMICMAIL: EncodingPrefix = 72;
    pub const APPLICATION_ATOMSVC_XML: EncodingPrefix = 73;
    pub const APPLICATION_ATSC_DWD_XML: EncodingPrefix = 74;
    pub const APPLICATION_ATSC_DYNAMIC_EVENT_MESSAGE: EncodingPrefix = 75;
    pub const APPLICATION_ATSC_HELD_XML: EncodingPrefix = 76;
    pub const APPLICATION_ATSC_RDT_JSON: EncodingPrefix = 77;
    pub const APPLICATION_ATSC_RSAT_XML: EncodingPrefix = 78;
    pub const APPLICATION_AUTH_POLICY_XML: EncodingPrefix = 79;
    pub const APPLICATION_AUTOMATIONML_AML_XML: EncodingPrefix = 80;
    pub const APPLICATION_AUTOMATIONML_AMLX_ZIP: EncodingPrefix = 81;
    pub const APPLICATION_BACNET_XDD_ZIP: EncodingPrefix = 82;
    pub const APPLICATION_BATCH_SMTP: EncodingPrefix = 83;
    pub const APPLICATION_BEEP_XML: EncodingPrefix = 84;
    pub const APPLICATION_C2PA: EncodingPrefix = 85;
    pub const APPLICATION_CALENDAR_JSON: EncodingPrefix = 86;
    pub const APPLICATION_CALENDAR_XML: EncodingPrefix = 87;
    pub const APPLICATION_CALL_COMPLETION: EncodingPrefix = 88;
    pub const APPLICATION_CAPTIVE_JSON: EncodingPrefix = 89;
    pub const APPLICATION_CBOR: EncodingPrefix = 90;
    pub const APPLICATION_CBOR_SEQ: EncodingPrefix = 91;
    pub const APPLICATION_CCCEX: EncodingPrefix = 92;
    pub const APPLICATION_CCMP_XML: EncodingPrefix = 93;
    pub const APPLICATION_CCXML_XML: EncodingPrefix = 94;
    pub const APPLICATION_CDA_XML: EncodingPrefix = 95;
    pub const APPLICATION_CDMI_CAPABILITY: EncodingPrefix = 96;
    pub const APPLICATION_CDMI_CONTAINER: EncodingPrefix = 97;
    pub const APPLICATION_CDMI_DOMAIN: EncodingPrefix = 98;
    pub const APPLICATION_CDMI_OBJECT: EncodingPrefix = 99;
    pub const APPLICATION_CDMI_QUEUE: EncodingPrefix = 100;
    pub const APPLICATION_CDNI: EncodingPrefix = 101;
    pub const APPLICATION_CEA_2018_XML: EncodingPrefix = 102;
    pub const APPLICATION_CELLML_XML: EncodingPrefix = 103;
    pub const APPLICATION_CFW: EncodingPrefix = 104;
    pub const APPLICATION_CID_EDHOC_CBOR_SEQ: EncodingPrefix = 105;
    pub const APPLICATION_CITY_JSON: EncodingPrefix = 106;
    pub const APPLICATION_CLR: EncodingPrefix = 107;
    pub const APPLICATION_CLUE_XML: EncodingPrefix = 108;
    pub const APPLICATION_CLUE_INFO_XML: EncodingPrefix = 109;
    pub const APPLICATION_CMS: EncodingPrefix = 110;
    pub const APPLICATION_CNRP_XML: EncodingPrefix = 111;
    pub const APPLICATION_COAP_GROUP_JSON: EncodingPrefix = 112;
    pub const APPLICATION_COAP_PAYLOAD: EncodingPrefix = 113;
    pub const APPLICATION_COMMONGROUND: EncodingPrefix = 114;
    pub const APPLICATION_CONCISE_PROBLEM_DETAILS_CBOR: EncodingPrefix = 115;
    pub const APPLICATION_CONFERENCE_INFO_XML: EncodingPrefix = 116;
    pub const APPLICATION_COSE: EncodingPrefix = 117;
    pub const APPLICATION_COSE_KEY: EncodingPrefix = 118;
    pub const APPLICATION_COSE_KEY_SET: EncodingPrefix = 119;
    pub const APPLICATION_COSE_X509: EncodingPrefix = 120;
    pub const APPLICATION_CPL_XML: EncodingPrefix = 121;
    pub const APPLICATION_CSRATTRS: EncodingPrefix = 122;
    pub const APPLICATION_CSTA_XML: EncodingPrefix = 123;
    pub const APPLICATION_CSVM_JSON: EncodingPrefix = 124;
    pub const APPLICATION_CWL: EncodingPrefix = 125;
    pub const APPLICATION_CWL_JSON: EncodingPrefix = 126;
    pub const APPLICATION_CWT: EncodingPrefix = 127;
    pub const APPLICATION_CYBERCASH: EncodingPrefix = 128;
    pub const APPLICATION_DASH_XML: EncodingPrefix = 129;
    pub const APPLICATION_DASH_PATCH_XML: EncodingPrefix = 130;
    pub const APPLICATION_DASHDELTA: EncodingPrefix = 131;
    pub const APPLICATION_DAVMOUNT_XML: EncodingPrefix = 132;
    pub const APPLICATION_DCA_RFT: EncodingPrefix = 133;
    pub const APPLICATION_DEC_DX: EncodingPrefix = 134;
    pub const APPLICATION_DIALOG_INFO_XML: EncodingPrefix = 135;
    pub const APPLICATION_DICOM: EncodingPrefix = 136;
    pub const APPLICATION_DICOM_JSON: EncodingPrefix = 137;
    pub const APPLICATION_DICOM_XML: EncodingPrefix = 138;
    pub const APPLICATION_DNS: EncodingPrefix = 139;
    pub const APPLICATION_DNS_JSON: EncodingPrefix = 140;
    pub const APPLICATION_DNS_MESSAGE: EncodingPrefix = 141;
    pub const APPLICATION_DOTS_CBOR: EncodingPrefix = 142;
    pub const APPLICATION_DPOP_JWT: EncodingPrefix = 143;
    pub const APPLICATION_DSKPP_XML: EncodingPrefix = 144;
    pub const APPLICATION_DSSC_DER: EncodingPrefix = 145;
    pub const APPLICATION_DSSC_XML: EncodingPrefix = 146;
    pub const APPLICATION_DVCS: EncodingPrefix = 147;
    pub const APPLICATION_ECMASCRIPT: EncodingPrefix = 148;
    pub const APPLICATION_EDHOC_CBOR_SEQ: EncodingPrefix = 149;
    pub const APPLICATION_EFI: EncodingPrefix = 150;
    pub const APPLICATION_ELM_JSON: EncodingPrefix = 151;
    pub const APPLICATION_ELM_XML: EncodingPrefix = 152;
    pub const APPLICATION_EMMA_XML: EncodingPrefix = 153;
    pub const APPLICATION_EMOTIONML_XML: EncodingPrefix = 154;
    pub const APPLICATION_ENCAPRTP: EncodingPrefix = 155;
    pub const APPLICATION_EPP_XML: EncodingPrefix = 156;
    pub const APPLICATION_EPUB_ZIP: EncodingPrefix = 157;
    pub const APPLICATION_ESHOP: EncodingPrefix = 158;
    pub const APPLICATION_EXAMPLE: EncodingPrefix = 159;
    pub const APPLICATION_EXI: EncodingPrefix = 160;
    pub const APPLICATION_EXPECT_CT_REPORT_JSON: EncodingPrefix = 161;
    pub const APPLICATION_EXPRESS: EncodingPrefix = 162;
    pub const APPLICATION_FASTINFOSET: EncodingPrefix = 163;
    pub const APPLICATION_FASTSOAP: EncodingPrefix = 164;
    pub const APPLICATION_FDF: EncodingPrefix = 165;
    pub const APPLICATION_FDT_XML: EncodingPrefix = 166;
    pub const APPLICATION_FHIR_JSON: EncodingPrefix = 167;
    pub const APPLICATION_FHIR_XML: EncodingPrefix = 168;
    pub const APPLICATION_FITS: EncodingPrefix = 169;
    pub const APPLICATION_FLEXFEC: EncodingPrefix = 170;
    pub const APPLICATION_FONT_SFNT: EncodingPrefix = 171;
    pub const APPLICATION_FONT_TDPFR: EncodingPrefix = 172;
    pub const APPLICATION_FONT_WOFF: EncodingPrefix = 173;
    pub const APPLICATION_FRAMEWORK_ATTRIBUTES_XML: EncodingPrefix = 174;
    pub const APPLICATION_GEO_JSON: EncodingPrefix = 175;
    pub const APPLICATION_GEO_JSON_SEQ: EncodingPrefix = 176;
    pub const APPLICATION_GEOPACKAGE_SQLITE3: EncodingPrefix = 177;
    pub const APPLICATION_GEOXACML_JSON: EncodingPrefix = 178;
    pub const APPLICATION_GEOXACML_XML: EncodingPrefix = 179;
    pub const APPLICATION_GLTF_BUFFER: EncodingPrefix = 180;
    pub const APPLICATION_GML_XML: EncodingPrefix = 181;
    pub const APPLICATION_GZIP: EncodingPrefix = 182;
    pub const APPLICATION_HELD_XML: EncodingPrefix = 183;
    pub const APPLICATION_HL7V2_XML: EncodingPrefix = 184;
    pub const APPLICATION_HTTP: EncodingPrefix = 185;
    pub const APPLICATION_HYPERSTUDIO: EncodingPrefix = 186;
    pub const APPLICATION_IBE_KEY_REQUEST_XML: EncodingPrefix = 187;
    pub const APPLICATION_IBE_PKG_REPLY_XML: EncodingPrefix = 188;
    pub const APPLICATION_IBE_PP_DATA: EncodingPrefix = 189;
    pub const APPLICATION_IGES: EncodingPrefix = 190;
    pub const APPLICATION_IM_ISCOMPOSING_XML: EncodingPrefix = 191;
    pub const APPLICATION_INDEX: EncodingPrefix = 192;
    pub const APPLICATION_INDEX_CMD: EncodingPrefix = 193;
    pub const APPLICATION_INDEX_OBJ: EncodingPrefix = 194;
    pub const APPLICATION_INDEX_RESPONSE: EncodingPrefix = 195;
    pub const APPLICATION_INDEX_VND: EncodingPrefix = 196;
    pub const APPLICATION_INKML_XML: EncodingPrefix = 197;
    pub const APPLICATION_IPFIX: EncodingPrefix = 198;
    pub const APPLICATION_IPP: EncodingPrefix = 199;
    pub const APPLICATION_ITS_XML: EncodingPrefix = 200;
    pub const APPLICATION_JAVA_ARCHIVE: EncodingPrefix = 201;
    pub const APPLICATION_JAVASCRIPT: EncodingPrefix = 202;
    pub const APPLICATION_JF2FEED_JSON: EncodingPrefix = 203;
    pub const APPLICATION_JOSE: EncodingPrefix = 204;
    pub const APPLICATION_JOSE_JSON: EncodingPrefix = 205;
    pub const APPLICATION_JRD_JSON: EncodingPrefix = 206;
    pub const APPLICATION_JSCALENDAR_JSON: EncodingPrefix = 207;
    pub const APPLICATION_JSCONTACT_JSON: EncodingPrefix = 208;
    pub const APPLICATION_JSON: EncodingPrefix = 209;
    pub const APPLICATION_JSON_PATCH_JSON: EncodingPrefix = 210;
    pub const APPLICATION_JSON_SEQ: EncodingPrefix = 211;
    pub const APPLICATION_JSONPATH: EncodingPrefix = 212;
    pub const APPLICATION_JWK_JSON: EncodingPrefix = 213;
    pub const APPLICATION_JWK_SET_JSON: EncodingPrefix = 214;
    pub const APPLICATION_JWT: EncodingPrefix = 215;
    pub const APPLICATION_KPML_REQUEST_XML: EncodingPrefix = 216;
    pub const APPLICATION_KPML_RESPONSE_XML: EncodingPrefix = 217;
    pub const APPLICATION_LD_JSON: EncodingPrefix = 218;
    pub const APPLICATION_LGR_XML: EncodingPrefix = 219;
    pub const APPLICATION_LINK_FORMAT: EncodingPrefix = 220;
    pub const APPLICATION_LINKSET: EncodingPrefix = 221;
    pub const APPLICATION_LINKSET_JSON: EncodingPrefix = 222;
    pub const APPLICATION_LOAD_CONTROL_XML: EncodingPrefix = 223;
    pub const APPLICATION_LOGOUT_JWT: EncodingPrefix = 224;
    pub const APPLICATION_LOST_XML: EncodingPrefix = 225;
    pub const APPLICATION_LOSTSYNC_XML: EncodingPrefix = 226;
    pub const APPLICATION_LPF_ZIP: EncodingPrefix = 227;
    pub const APPLICATION_MAC_BINHEX40: EncodingPrefix = 228;
    pub const APPLICATION_MACWRITEII: EncodingPrefix = 229;
    pub const APPLICATION_MADS_XML: EncodingPrefix = 230;
    pub const APPLICATION_MANIFEST_JSON: EncodingPrefix = 231;
    pub const APPLICATION_MARC: EncodingPrefix = 232;
    pub const APPLICATION_MARCXML_XML: EncodingPrefix = 233;
    pub const APPLICATION_MATHEMATICA: EncodingPrefix = 234;
    pub const APPLICATION_MATHML_XML: EncodingPrefix = 235;
    pub const APPLICATION_MATHML_CONTENT_XML: EncodingPrefix = 236;
    pub const APPLICATION_MATHML_PRESENTATION_XML: EncodingPrefix = 237;
    pub const APPLICATION_MBMS_ASSOCIATED_PROCEDURE_DESCRIPTION_XML: EncodingPrefix = 238;
    pub const APPLICATION_MBMS_DEREGISTER_XML: EncodingPrefix = 239;
    pub const APPLICATION_MBMS_ENVELOPE_XML: EncodingPrefix = 240;
    pub const APPLICATION_MBMS_MSK_XML: EncodingPrefix = 241;
    pub const APPLICATION_MBMS_MSK_RESPONSE_XML: EncodingPrefix = 242;
    pub const APPLICATION_MBMS_PROTECTION_DESCRIPTION_XML: EncodingPrefix = 243;
    pub const APPLICATION_MBMS_RECEPTION_REPORT_XML: EncodingPrefix = 244;
    pub const APPLICATION_MBMS_REGISTER_XML: EncodingPrefix = 245;
    pub const APPLICATION_MBMS_REGISTER_RESPONSE_XML: EncodingPrefix = 246;
    pub const APPLICATION_MBMS_SCHEDULE_XML: EncodingPrefix = 247;
    pub const APPLICATION_MBMS_USER_SERVICE_DESCRIPTION_XML: EncodingPrefix = 248;
    pub const APPLICATION_MBOX: EncodingPrefix = 249;
    pub const APPLICATION_MEDIA_POLICY_DATASET_XML: EncodingPrefix = 250;
    pub const APPLICATION_MEDIA_CONTROL_XML: EncodingPrefix = 251;
    pub const APPLICATION_MEDIASERVERCONTROL_XML: EncodingPrefix = 252;
    pub const APPLICATION_MERGE_PATCH_JSON: EncodingPrefix = 253;
    pub const APPLICATION_METALINK4_XML: EncodingPrefix = 254;
    pub const APPLICATION_METS_XML: EncodingPrefix = 255;
    pub const APPLICATION_MIKEY: EncodingPrefix = 256;
    pub const APPLICATION_MIPC: EncodingPrefix = 257;
    pub const APPLICATION_MISSING_BLOCKS_CBOR_SEQ: EncodingPrefix = 258;
    pub const APPLICATION_MMT_AEI_XML: EncodingPrefix = 259;
    pub const APPLICATION_MMT_USD_XML: EncodingPrefix = 260;
    pub const APPLICATION_MODS_XML: EncodingPrefix = 261;
    pub const APPLICATION_MOSS_KEYS: EncodingPrefix = 262;
    pub const APPLICATION_MOSS_SIGNATURE: EncodingPrefix = 263;
    pub const APPLICATION_MOSSKEY_DATA: EncodingPrefix = 264;
    pub const APPLICATION_MOSSKEY_REQUEST: EncodingPrefix = 265;
    pub const APPLICATION_MP21: EncodingPrefix = 266;
    pub const APPLICATION_MP4: EncodingPrefix = 267;
    pub const APPLICATION_MPEG4_GENERIC: EncodingPrefix = 268;
    pub const APPLICATION_MPEG4_IOD: EncodingPrefix = 269;
    pub const APPLICATION_MPEG4_IOD_XMT: EncodingPrefix = 270;
    pub const APPLICATION_MRB_CONSUMER_XML: EncodingPrefix = 271;
    pub const APPLICATION_MRB_PUBLISH_XML: EncodingPrefix = 272;
    pub const APPLICATION_MSC_IVR_XML: EncodingPrefix = 273;
    pub const APPLICATION_MSC_MIXER_XML: EncodingPrefix = 274;
    pub const APPLICATION_MSWORD: EncodingPrefix = 275;
    pub const APPLICATION_MUD_JSON: EncodingPrefix = 276;
    pub const APPLICATION_MULTIPART_CORE: EncodingPrefix = 277;
    pub const APPLICATION_MXF: EncodingPrefix = 278;
    pub const APPLICATION_N_QUADS: EncodingPrefix = 279;
    pub const APPLICATION_N_TRIPLES: EncodingPrefix = 280;
    pub const APPLICATION_NASDATA: EncodingPrefix = 281;
    pub const APPLICATION_NEWS_CHECKGROUPS: EncodingPrefix = 282;
    pub const APPLICATION_NEWS_GROUPINFO: EncodingPrefix = 283;
    pub const APPLICATION_NEWS_TRANSMISSION: EncodingPrefix = 284;
    pub const APPLICATION_NLSML_XML: EncodingPrefix = 285;
    pub const APPLICATION_NODE: EncodingPrefix = 286;
    pub const APPLICATION_NSS: EncodingPrefix = 287;
    pub const APPLICATION_OAUTH_AUTHZ_REQ_JWT: EncodingPrefix = 288;
    pub const APPLICATION_OBLIVIOUS_DNS_MESSAGE: EncodingPrefix = 289;
    pub const APPLICATION_OCSP_REQUEST: EncodingPrefix = 290;
    pub const APPLICATION_OCSP_RESPONSE: EncodingPrefix = 291;
    pub const APPLICATION_OCTET_STREAM: EncodingPrefix = 292;
    pub const APPLICATION_ODM_XML: EncodingPrefix = 293;
    pub const APPLICATION_OEBPS_PACKAGE_XML: EncodingPrefix = 294;
    pub const APPLICATION_OGG: EncodingPrefix = 295;
    pub const APPLICATION_OHTTP_KEYS: EncodingPrefix = 296;
    pub const APPLICATION_OPC_NODESET_XML: EncodingPrefix = 297;
    pub const APPLICATION_OSCORE: EncodingPrefix = 298;
    pub const APPLICATION_OXPS: EncodingPrefix = 299;
    pub const APPLICATION_P21: EncodingPrefix = 300;
    pub const APPLICATION_P21_ZIP: EncodingPrefix = 301;
    pub const APPLICATION_P2P_OVERLAY_XML: EncodingPrefix = 302;
    pub const APPLICATION_PARITYFEC: EncodingPrefix = 303;
    pub const APPLICATION_PASSPORT: EncodingPrefix = 304;
    pub const APPLICATION_PATCH_OPS_ERROR_XML: EncodingPrefix = 305;
    pub const APPLICATION_PDF: EncodingPrefix = 306;
    pub const APPLICATION_PEM_CERTIFICATE_CHAIN: EncodingPrefix = 307;
    pub const APPLICATION_PGP_ENCRYPTED: EncodingPrefix = 308;
    pub const APPLICATION_PGP_KEYS: EncodingPrefix = 309;
    pub const APPLICATION_PGP_SIGNATURE: EncodingPrefix = 310;
    pub const APPLICATION_PIDF_XML: EncodingPrefix = 311;
    pub const APPLICATION_PIDF_DIFF_XML: EncodingPrefix = 312;
    pub const APPLICATION_PKCS10: EncodingPrefix = 313;
    pub const APPLICATION_PKCS12: EncodingPrefix = 314;
    pub const APPLICATION_PKCS7_MIME: EncodingPrefix = 315;
    pub const APPLICATION_PKCS7_SIGNATURE: EncodingPrefix = 316;
    pub const APPLICATION_PKCS8: EncodingPrefix = 317;
    pub const APPLICATION_PKCS8_ENCRYPTED: EncodingPrefix = 318;
    pub const APPLICATION_PKIX_ATTR_CERT: EncodingPrefix = 319;
    pub const APPLICATION_PKIX_CERT: EncodingPrefix = 320;
    pub const APPLICATION_PKIX_CRL: EncodingPrefix = 321;
    pub const APPLICATION_PKIX_PKIPATH: EncodingPrefix = 322;
    pub const APPLICATION_PKIXCMP: EncodingPrefix = 323;
    pub const APPLICATION_PLS_XML: EncodingPrefix = 324;
    pub const APPLICATION_POC_SETTINGS_XML: EncodingPrefix = 325;
    pub const APPLICATION_POSTSCRIPT: EncodingPrefix = 326;
    pub const APPLICATION_PPSP_TRACKER_JSON: EncodingPrefix = 327;
    pub const APPLICATION_PRIVATE_TOKEN_ISSUER_DIRECTORY: EncodingPrefix = 328;
    pub const APPLICATION_PRIVATE_TOKEN_REQUEST: EncodingPrefix = 329;
    pub const APPLICATION_PRIVATE_TOKEN_RESPONSE: EncodingPrefix = 330;
    pub const APPLICATION_PROBLEM_JSON: EncodingPrefix = 331;
    pub const APPLICATION_PROBLEM_XML: EncodingPrefix = 332;
    pub const APPLICATION_PROVENANCE_XML: EncodingPrefix = 333;
    pub const APPLICATION_PRS_ALVESTRAND_TITRAX_SHEET: EncodingPrefix = 334;
    pub const APPLICATION_PRS_CWW: EncodingPrefix = 335;
    pub const APPLICATION_PRS_CYN: EncodingPrefix = 336;
    pub const APPLICATION_PRS_HPUB_ZIP: EncodingPrefix = 337;
    pub const APPLICATION_PRS_IMPLIED_DOCUMENT_XML: EncodingPrefix = 338;
    pub const APPLICATION_PRS_IMPLIED_EXECUTABLE: EncodingPrefix = 339;
    pub const APPLICATION_PRS_IMPLIED_OBJECT_JSON: EncodingPrefix = 340;
    pub const APPLICATION_PRS_IMPLIED_OBJECT_JSON_SEQ: EncodingPrefix = 341;
    pub const APPLICATION_PRS_IMPLIED_OBJECT_YAML: EncodingPrefix = 342;
    pub const APPLICATION_PRS_IMPLIED_STRUCTURE: EncodingPrefix = 343;
    pub const APPLICATION_PRS_NPREND: EncodingPrefix = 344;
    pub const APPLICATION_PRS_PLUCKER: EncodingPrefix = 345;
    pub const APPLICATION_PRS_RDF_XML_CRYPT: EncodingPrefix = 346;
    pub const APPLICATION_PRS_VCFBZIP2: EncodingPrefix = 347;
    pub const APPLICATION_PRS_XSF_XML: EncodingPrefix = 348;
    pub const APPLICATION_PSKC_XML: EncodingPrefix = 349;
    pub const APPLICATION_PVD_JSON: EncodingPrefix = 350;
    pub const APPLICATION_RAPTORFEC: EncodingPrefix = 351;
    pub const APPLICATION_RDAP_JSON: EncodingPrefix = 352;
    pub const APPLICATION_RDF_XML: EncodingPrefix = 353;
    pub const APPLICATION_REGINFO_XML: EncodingPrefix = 354;
    pub const APPLICATION_RELAX_NG_COMPACT_SYNTAX: EncodingPrefix = 355;
    pub const APPLICATION_REMOTE_PRINTING: EncodingPrefix = 356;
    pub const APPLICATION_REPUTON_JSON: EncodingPrefix = 357;
    pub const APPLICATION_RESOURCE_LISTS_XML: EncodingPrefix = 358;
    pub const APPLICATION_RESOURCE_LISTS_DIFF_XML: EncodingPrefix = 359;
    pub const APPLICATION_RFC_XML: EncodingPrefix = 360;
    pub const APPLICATION_RISCOS: EncodingPrefix = 361;
    pub const APPLICATION_RLMI_XML: EncodingPrefix = 362;
    pub const APPLICATION_RLS_SERVICES_XML: EncodingPrefix = 363;
    pub const APPLICATION_ROUTE_APD_XML: EncodingPrefix = 364;
    pub const APPLICATION_ROUTE_S_TSID_XML: EncodingPrefix = 365;
    pub const APPLICATION_ROUTE_USD_XML: EncodingPrefix = 366;
    pub const APPLICATION_RPKI_CHECKLIST: EncodingPrefix = 367;
    pub const APPLICATION_RPKI_GHOSTBUSTERS: EncodingPrefix = 368;
    pub const APPLICATION_RPKI_MANIFEST: EncodingPrefix = 369;
    pub const APPLICATION_RPKI_PUBLICATION: EncodingPrefix = 370;
    pub const APPLICATION_RPKI_ROA: EncodingPrefix = 371;
    pub const APPLICATION_RPKI_UPDOWN: EncodingPrefix = 372;
    pub const APPLICATION_RTF: EncodingPrefix = 373;
    pub const APPLICATION_RTPLOOPBACK: EncodingPrefix = 374;
    pub const APPLICATION_RTX: EncodingPrefix = 375;
    pub const APPLICATION_SAMLASSERTION_XML: EncodingPrefix = 376;
    pub const APPLICATION_SAMLMETADATA_XML: EncodingPrefix = 377;
    pub const APPLICATION_SARIF_JSON: EncodingPrefix = 378;
    pub const APPLICATION_SARIF_EXTERNAL_PROPERTIES_JSON: EncodingPrefix = 379;
    pub const APPLICATION_SBE: EncodingPrefix = 380;
    pub const APPLICATION_SBML_XML: EncodingPrefix = 381;
    pub const APPLICATION_SCAIP_XML: EncodingPrefix = 382;
    pub const APPLICATION_SCIM_JSON: EncodingPrefix = 383;
    pub const APPLICATION_SCVP_CV_REQUEST: EncodingPrefix = 384;
    pub const APPLICATION_SCVP_CV_RESPONSE: EncodingPrefix = 385;
    pub const APPLICATION_SCVP_VP_REQUEST: EncodingPrefix = 386;
    pub const APPLICATION_SCVP_VP_RESPONSE: EncodingPrefix = 387;
    pub const APPLICATION_SDP: EncodingPrefix = 388;
    pub const APPLICATION_SECEVENT_JWT: EncodingPrefix = 389;
    pub const APPLICATION_SENML_CBOR: EncodingPrefix = 390;
    pub const APPLICATION_SENML_JSON: EncodingPrefix = 391;
    pub const APPLICATION_SENML_XML: EncodingPrefix = 392;
    pub const APPLICATION_SENML_ETCH_CBOR: EncodingPrefix = 393;
    pub const APPLICATION_SENML_ETCH_JSON: EncodingPrefix = 394;
    pub const APPLICATION_SENML_EXI: EncodingPrefix = 395;
    pub const APPLICATION_SENSML_CBOR: EncodingPrefix = 396;
    pub const APPLICATION_SENSML_JSON: EncodingPrefix = 397;
    pub const APPLICATION_SENSML_XML: EncodingPrefix = 398;
    pub const APPLICATION_SENSML_EXI: EncodingPrefix = 399;
    pub const APPLICATION_SEP_XML: EncodingPrefix = 400;
    pub const APPLICATION_SEP_EXI: EncodingPrefix = 401;
    pub const APPLICATION_SESSION_INFO: EncodingPrefix = 402;
    pub const APPLICATION_SET_PAYMENT: EncodingPrefix = 403;
    pub const APPLICATION_SET_PAYMENT_INITIATION: EncodingPrefix = 404;
    pub const APPLICATION_SET_REGISTRATION: EncodingPrefix = 405;
    pub const APPLICATION_SET_REGISTRATION_INITIATION: EncodingPrefix = 406;
    pub const APPLICATION_SGML_OPEN_CATALOG: EncodingPrefix = 407;
    pub const APPLICATION_SHF_XML: EncodingPrefix = 408;
    pub const APPLICATION_SIEVE: EncodingPrefix = 409;
    pub const APPLICATION_SIMPLE_FILTER_XML: EncodingPrefix = 410;
    pub const APPLICATION_SIMPLE_MESSAGE_SUMMARY: EncodingPrefix = 411;
    pub const APPLICATION_SIMPLESYMBOLCONTAINER: EncodingPrefix = 412;
    pub const APPLICATION_SIPC: EncodingPrefix = 413;
    pub const APPLICATION_SLATE: EncodingPrefix = 414;
    pub const APPLICATION_SMIL: EncodingPrefix = 415;
    pub const APPLICATION_SMIL_XML: EncodingPrefix = 416;
    pub const APPLICATION_SMPTE336M: EncodingPrefix = 417;
    pub const APPLICATION_SOAP_FASTINFOSET: EncodingPrefix = 418;
    pub const APPLICATION_SOAP_XML: EncodingPrefix = 419;
    pub const APPLICATION_SPARQL_QUERY: EncodingPrefix = 420;
    pub const APPLICATION_SPARQL_RESULTS_XML: EncodingPrefix = 421;
    pub const APPLICATION_SPDX_JSON: EncodingPrefix = 422;
    pub const APPLICATION_SPIRITS_EVENT_XML: EncodingPrefix = 423;
    pub const APPLICATION_SQL: EncodingPrefix = 424;
    pub const APPLICATION_SRGS: EncodingPrefix = 425;
    pub const APPLICATION_SRGS_XML: EncodingPrefix = 426;
    pub const APPLICATION_SRU_XML: EncodingPrefix = 427;
    pub const APPLICATION_SSML_XML: EncodingPrefix = 428;
    pub const APPLICATION_STIX_JSON: EncodingPrefix = 429;
    pub const APPLICATION_SWID_CBOR: EncodingPrefix = 430;
    pub const APPLICATION_SWID_XML: EncodingPrefix = 431;
    pub const APPLICATION_TAMP_APEX_UPDATE: EncodingPrefix = 432;
    pub const APPLICATION_TAMP_APEX_UPDATE_CONFIRM: EncodingPrefix = 433;
    pub const APPLICATION_TAMP_COMMUNITY_UPDATE: EncodingPrefix = 434;
    pub const APPLICATION_TAMP_COMMUNITY_UPDATE_CONFIRM: EncodingPrefix = 435;
    pub const APPLICATION_TAMP_ERROR: EncodingPrefix = 436;
    pub const APPLICATION_TAMP_SEQUENCE_ADJUST: EncodingPrefix = 437;
    pub const APPLICATION_TAMP_SEQUENCE_ADJUST_CONFIRM: EncodingPrefix = 438;
    pub const APPLICATION_TAMP_STATUS_QUERY: EncodingPrefix = 439;
    pub const APPLICATION_TAMP_STATUS_RESPONSE: EncodingPrefix = 440;
    pub const APPLICATION_TAMP_UPDATE: EncodingPrefix = 441;
    pub const APPLICATION_TAMP_UPDATE_CONFIRM: EncodingPrefix = 442;
    pub const APPLICATION_TAXII_JSON: EncodingPrefix = 443;
    pub const APPLICATION_TD_JSON: EncodingPrefix = 444;
    pub const APPLICATION_TEI_XML: EncodingPrefix = 445;
    pub const APPLICATION_THRAUD_XML: EncodingPrefix = 446;
    pub const APPLICATION_TIMESTAMP_QUERY: EncodingPrefix = 447;
    pub const APPLICATION_TIMESTAMP_REPLY: EncodingPrefix = 448;
    pub const APPLICATION_TIMESTAMPED_DATA: EncodingPrefix = 449;
    pub const APPLICATION_TLSRPT_GZIP: EncodingPrefix = 450;
    pub const APPLICATION_TLSRPT_JSON: EncodingPrefix = 451;
    pub const APPLICATION_TM_JSON: EncodingPrefix = 452;
    pub const APPLICATION_TNAUTHLIST: EncodingPrefix = 453;
    pub const APPLICATION_TOKEN_INTROSPECTION_JWT: EncodingPrefix = 454;
    pub const APPLICATION_TRICKLE_ICE_SDPFRAG: EncodingPrefix = 455;
    pub const APPLICATION_TRIG: EncodingPrefix = 456;
    pub const APPLICATION_TTML_XML: EncodingPrefix = 457;
    pub const APPLICATION_TVE_TRIGGER: EncodingPrefix = 458;
    pub const APPLICATION_TZIF: EncodingPrefix = 459;
    pub const APPLICATION_TZIF_LEAP: EncodingPrefix = 460;
    pub const APPLICATION_ULPFEC: EncodingPrefix = 461;
    pub const APPLICATION_URC_GRPSHEET_XML: EncodingPrefix = 462;
    pub const APPLICATION_URC_RESSHEET_XML: EncodingPrefix = 463;
    pub const APPLICATION_URC_TARGETDESC_XML: EncodingPrefix = 464;
    pub const APPLICATION_URC_UISOCKETDESC_XML: EncodingPrefix = 465;
    pub const APPLICATION_VCARD_JSON: EncodingPrefix = 466;
    pub const APPLICATION_VCARD_XML: EncodingPrefix = 467;
    pub const APPLICATION_VEMMI: EncodingPrefix = 468;
    pub const APPLICATION_VND_1000MINDS_DECISION_MODEL_XML: EncodingPrefix = 469;
    pub const APPLICATION_VND_1OB: EncodingPrefix = 470;
    pub const APPLICATION_VND_3M_POST_IT_NOTES: EncodingPrefix = 471;
    pub const APPLICATION_VND_3GPP_PROSE_XML: EncodingPrefix = 472;
    pub const APPLICATION_VND_3GPP_PROSE_PC3A_XML: EncodingPrefix = 473;
    pub const APPLICATION_VND_3GPP_PROSE_PC3ACH_XML: EncodingPrefix = 474;
    pub const APPLICATION_VND_3GPP_PROSE_PC3CH_XML: EncodingPrefix = 475;
    pub const APPLICATION_VND_3GPP_PROSE_PC8_XML: EncodingPrefix = 476;
    pub const APPLICATION_VND_3GPP_V2X_LOCAL_SERVICE_INFORMATION: EncodingPrefix = 477;
    pub const APPLICATION_VND_3GPP_5GNAS: EncodingPrefix = 478;
    pub const APPLICATION_VND_3GPP_GMOP_XML: EncodingPrefix = 479;
    pub const APPLICATION_VND_3GPP_SRVCC_INFO_XML: EncodingPrefix = 480;
    pub const APPLICATION_VND_3GPP_ACCESS_TRANSFER_EVENTS_XML: EncodingPrefix = 481;
    pub const APPLICATION_VND_3GPP_BSF_XML: EncodingPrefix = 482;
    pub const APPLICATION_VND_3GPP_CRS_XML: EncodingPrefix = 483;
    pub const APPLICATION_VND_3GPP_CURRENT_LOCATION_DISCOVERY_XML: EncodingPrefix = 484;
    pub const APPLICATION_VND_3GPP_GTPC: EncodingPrefix = 485;
    pub const APPLICATION_VND_3GPP_INTERWORKING_DATA: EncodingPrefix = 486;
    pub const APPLICATION_VND_3GPP_LPP: EncodingPrefix = 487;
    pub const APPLICATION_VND_3GPP_MC_SIGNALLING_EAR: EncodingPrefix = 488;
    pub const APPLICATION_VND_3GPP_MCDATA_AFFILIATION_COMMAND_XML: EncodingPrefix = 489;
    pub const APPLICATION_VND_3GPP_MCDATA_INFO_XML: EncodingPrefix = 490;
    pub const APPLICATION_VND_3GPP_MCDATA_MSGSTORE_CTRL_REQUEST_XML: EncodingPrefix = 491;
    pub const APPLICATION_VND_3GPP_MCDATA_PAYLOAD: EncodingPrefix = 492;
    pub const APPLICATION_VND_3GPP_MCDATA_REGROUP_XML: EncodingPrefix = 493;
    pub const APPLICATION_VND_3GPP_MCDATA_SERVICE_CONFIG_XML: EncodingPrefix = 494;
    pub const APPLICATION_VND_3GPP_MCDATA_SIGNALLING: EncodingPrefix = 495;
    pub const APPLICATION_VND_3GPP_MCDATA_UE_CONFIG_XML: EncodingPrefix = 496;
    pub const APPLICATION_VND_3GPP_MCDATA_USER_PROFILE_XML: EncodingPrefix = 497;
    pub const APPLICATION_VND_3GPP_MCPTT_AFFILIATION_COMMAND_XML: EncodingPrefix = 498;
    pub const APPLICATION_VND_3GPP_MCPTT_FLOOR_REQUEST_XML: EncodingPrefix = 499;
    pub const APPLICATION_VND_3GPP_MCPTT_INFO_XML: EncodingPrefix = 500;
    pub const APPLICATION_VND_3GPP_MCPTT_LOCATION_INFO_XML: EncodingPrefix = 501;
    pub const APPLICATION_VND_3GPP_MCPTT_MBMS_USAGE_INFO_XML: EncodingPrefix = 502;
    pub const APPLICATION_VND_3GPP_MCPTT_REGROUP_XML: EncodingPrefix = 503;
    pub const APPLICATION_VND_3GPP_MCPTT_SERVICE_CONFIG_XML: EncodingPrefix = 504;
    pub const APPLICATION_VND_3GPP_MCPTT_SIGNED_XML: EncodingPrefix = 505;
    pub const APPLICATION_VND_3GPP_MCPTT_UE_CONFIG_XML: EncodingPrefix = 506;
    pub const APPLICATION_VND_3GPP_MCPTT_UE_INIT_CONFIG_XML: EncodingPrefix = 507;
    pub const APPLICATION_VND_3GPP_MCPTT_USER_PROFILE_XML: EncodingPrefix = 508;
    pub const APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_COMMAND_XML: EncodingPrefix = 509;
    pub const APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_INFO_XML: EncodingPrefix = 510;
    pub const APPLICATION_VND_3GPP_MCVIDEO_INFO_XML: EncodingPrefix = 511;
    pub const APPLICATION_VND_3GPP_MCVIDEO_LOCATION_INFO_XML: EncodingPrefix = 512;
    pub const APPLICATION_VND_3GPP_MCVIDEO_MBMS_USAGE_INFO_XML: EncodingPrefix = 513;
    pub const APPLICATION_VND_3GPP_MCVIDEO_REGROUP_XML: EncodingPrefix = 514;
    pub const APPLICATION_VND_3GPP_MCVIDEO_SERVICE_CONFIG_XML: EncodingPrefix = 515;
    pub const APPLICATION_VND_3GPP_MCVIDEO_TRANSMISSION_REQUEST_XML: EncodingPrefix = 516;
    pub const APPLICATION_VND_3GPP_MCVIDEO_UE_CONFIG_XML: EncodingPrefix = 517;
    pub const APPLICATION_VND_3GPP_MCVIDEO_USER_PROFILE_XML: EncodingPrefix = 518;
    pub const APPLICATION_VND_3GPP_MID_CALL_XML: EncodingPrefix = 519;
    pub const APPLICATION_VND_3GPP_NGAP: EncodingPrefix = 520;
    pub const APPLICATION_VND_3GPP_PFCP: EncodingPrefix = 521;
    pub const APPLICATION_VND_3GPP_PIC_BW_LARGE: EncodingPrefix = 522;
    pub const APPLICATION_VND_3GPP_PIC_BW_SMALL: EncodingPrefix = 523;
    pub const APPLICATION_VND_3GPP_PIC_BW_VAR: EncodingPrefix = 524;
    pub const APPLICATION_VND_3GPP_S1AP: EncodingPrefix = 525;
    pub const APPLICATION_VND_3GPP_SEAL_GROUP_DOC_XML: EncodingPrefix = 526;
    pub const APPLICATION_VND_3GPP_SEAL_INFO_XML: EncodingPrefix = 527;
    pub const APPLICATION_VND_3GPP_SEAL_LOCATION_INFO_XML: EncodingPrefix = 528;
    pub const APPLICATION_VND_3GPP_SEAL_MBMS_USAGE_INFO_XML: EncodingPrefix = 529;
    pub const APPLICATION_VND_3GPP_SEAL_NETWORK_QOS_MANAGEMENT_INFO_XML: EncodingPrefix = 530;
    pub const APPLICATION_VND_3GPP_SEAL_UE_CONFIG_INFO_XML: EncodingPrefix = 531;
    pub const APPLICATION_VND_3GPP_SEAL_UNICAST_INFO_XML: EncodingPrefix = 532;
    pub const APPLICATION_VND_3GPP_SEAL_USER_PROFILE_INFO_XML: EncodingPrefix = 533;
    pub const APPLICATION_VND_3GPP_SMS: EncodingPrefix = 534;
    pub const APPLICATION_VND_3GPP_SMS_XML: EncodingPrefix = 535;
    pub const APPLICATION_VND_3GPP_SRVCC_EXT_XML: EncodingPrefix = 536;
    pub const APPLICATION_VND_3GPP_STATE_AND_EVENT_INFO_XML: EncodingPrefix = 537;
    pub const APPLICATION_VND_3GPP_USSD_XML: EncodingPrefix = 538;
    pub const APPLICATION_VND_3GPP_V2X: EncodingPrefix = 539;
    pub const APPLICATION_VND_3GPP_VAE_INFO_XML: EncodingPrefix = 540;
    pub const APPLICATION_VND_3GPP2_BCMCSINFO_XML: EncodingPrefix = 541;
    pub const APPLICATION_VND_3GPP2_SMS: EncodingPrefix = 542;
    pub const APPLICATION_VND_3GPP2_TCAP: EncodingPrefix = 543;
    pub const APPLICATION_VND_3LIGHTSSOFTWARE_IMAGESCAL: EncodingPrefix = 544;
    pub const APPLICATION_VND_FLOGRAPHIT: EncodingPrefix = 545;
    pub const APPLICATION_VND_HANDHELD_ENTERTAINMENT_XML: EncodingPrefix = 546;
    pub const APPLICATION_VND_KINAR: EncodingPrefix = 547;
    pub const APPLICATION_VND_MFER: EncodingPrefix = 548;
    pub const APPLICATION_VND_MOBIUS_DAF: EncodingPrefix = 549;
    pub const APPLICATION_VND_MOBIUS_DIS: EncodingPrefix = 550;
    pub const APPLICATION_VND_MOBIUS_MBK: EncodingPrefix = 551;
    pub const APPLICATION_VND_MOBIUS_MQY: EncodingPrefix = 552;
    pub const APPLICATION_VND_MOBIUS_MSL: EncodingPrefix = 553;
    pub const APPLICATION_VND_MOBIUS_PLC: EncodingPrefix = 554;
    pub const APPLICATION_VND_MOBIUS_TXF: EncodingPrefix = 555;
    pub const APPLICATION_VND_QUARK_QUARKXPRESS: EncodingPrefix = 556;
    pub const APPLICATION_VND_RENLEARN_RLPRINT: EncodingPrefix = 557;
    pub const APPLICATION_VND_SIMTECH_MINDMAPPER: EncodingPrefix = 558;
    pub const APPLICATION_VND_ACCPAC_SIMPLY_ASO: EncodingPrefix = 559;
    pub const APPLICATION_VND_ACCPAC_SIMPLY_IMP: EncodingPrefix = 560;
    pub const APPLICATION_VND_ACM_ADDRESSXFER_JSON: EncodingPrefix = 561;
    pub const APPLICATION_VND_ACM_CHATBOT_JSON: EncodingPrefix = 562;
    pub const APPLICATION_VND_ACUCOBOL: EncodingPrefix = 563;
    pub const APPLICATION_VND_ACUCORP: EncodingPrefix = 564;
    pub const APPLICATION_VND_ADOBE_FLASH_MOVIE: EncodingPrefix = 565;
    pub const APPLICATION_VND_ADOBE_FORMSCENTRAL_FCDT: EncodingPrefix = 566;
    pub const APPLICATION_VND_ADOBE_FXP: EncodingPrefix = 567;
    pub const APPLICATION_VND_ADOBE_PARTIAL_UPLOAD: EncodingPrefix = 568;
    pub const APPLICATION_VND_ADOBE_XDP_XML: EncodingPrefix = 569;
    pub const APPLICATION_VND_AETHER_IMP: EncodingPrefix = 570;
    pub const APPLICATION_VND_AFPC_AFPLINEDATA: EncodingPrefix = 571;
    pub const APPLICATION_VND_AFPC_AFPLINEDATA_PAGEDEF: EncodingPrefix = 572;
    pub const APPLICATION_VND_AFPC_CMOCA_CMRESOURCE: EncodingPrefix = 573;
    pub const APPLICATION_VND_AFPC_FOCA_CHARSET: EncodingPrefix = 574;
    pub const APPLICATION_VND_AFPC_FOCA_CODEDFONT: EncodingPrefix = 575;
    pub const APPLICATION_VND_AFPC_FOCA_CODEPAGE: EncodingPrefix = 576;
    pub const APPLICATION_VND_AFPC_MODCA: EncodingPrefix = 577;
    pub const APPLICATION_VND_AFPC_MODCA_CMTABLE: EncodingPrefix = 578;
    pub const APPLICATION_VND_AFPC_MODCA_FORMDEF: EncodingPrefix = 579;
    pub const APPLICATION_VND_AFPC_MODCA_MEDIUMMAP: EncodingPrefix = 580;
    pub const APPLICATION_VND_AFPC_MODCA_OBJECTCONTAINER: EncodingPrefix = 581;
    pub const APPLICATION_VND_AFPC_MODCA_OVERLAY: EncodingPrefix = 582;
    pub const APPLICATION_VND_AFPC_MODCA_PAGESEGMENT: EncodingPrefix = 583;
    pub const APPLICATION_VND_AGE: EncodingPrefix = 584;
    pub const APPLICATION_VND_AH_BARCODE: EncodingPrefix = 585;
    pub const APPLICATION_VND_AHEAD_SPACE: EncodingPrefix = 586;
    pub const APPLICATION_VND_AIRZIP_FILESECURE_AZF: EncodingPrefix = 587;
    pub const APPLICATION_VND_AIRZIP_FILESECURE_AZS: EncodingPrefix = 588;
    pub const APPLICATION_VND_AMADEUS_JSON: EncodingPrefix = 589;
    pub const APPLICATION_VND_AMAZON_MOBI8_EBOOK: EncodingPrefix = 590;
    pub const APPLICATION_VND_AMERICANDYNAMICS_ACC: EncodingPrefix = 591;
    pub const APPLICATION_VND_AMIGA_AMI: EncodingPrefix = 592;
    pub const APPLICATION_VND_AMUNDSEN_MAZE_XML: EncodingPrefix = 593;
    pub const APPLICATION_VND_ANDROID_OTA: EncodingPrefix = 594;
    pub const APPLICATION_VND_ANKI: EncodingPrefix = 595;
    pub const APPLICATION_VND_ANSER_WEB_CERTIFICATE_ISSUE_INITIATION: EncodingPrefix = 596;
    pub const APPLICATION_VND_ANTIX_GAME_COMPONENT: EncodingPrefix = 597;
    pub const APPLICATION_VND_APACHE_ARROW_FILE: EncodingPrefix = 598;
    pub const APPLICATION_VND_APACHE_ARROW_STREAM: EncodingPrefix = 599;
    pub const APPLICATION_VND_APACHE_PARQUET: EncodingPrefix = 600;
    pub const APPLICATION_VND_APACHE_THRIFT_BINARY: EncodingPrefix = 601;
    pub const APPLICATION_VND_APACHE_THRIFT_COMPACT: EncodingPrefix = 602;
    pub const APPLICATION_VND_APACHE_THRIFT_JSON: EncodingPrefix = 603;
    pub const APPLICATION_VND_APEXLANG: EncodingPrefix = 604;
    pub const APPLICATION_VND_API_JSON: EncodingPrefix = 605;
    pub const APPLICATION_VND_APLEXTOR_WARRP_JSON: EncodingPrefix = 606;
    pub const APPLICATION_VND_APOTHEKENDE_RESERVATION_JSON: EncodingPrefix = 607;
    pub const APPLICATION_VND_APPLE_INSTALLER_XML: EncodingPrefix = 608;
    pub const APPLICATION_VND_APPLE_KEYNOTE: EncodingPrefix = 609;
    pub const APPLICATION_VND_APPLE_MPEGURL: EncodingPrefix = 610;
    pub const APPLICATION_VND_APPLE_NUMBERS: EncodingPrefix = 611;
    pub const APPLICATION_VND_APPLE_PAGES: EncodingPrefix = 612;
    pub const APPLICATION_VND_ARASTRA_SWI: EncodingPrefix = 613;
    pub const APPLICATION_VND_ARISTANETWORKS_SWI: EncodingPrefix = 614;
    pub const APPLICATION_VND_ARTISAN_JSON: EncodingPrefix = 615;
    pub const APPLICATION_VND_ARTSQUARE: EncodingPrefix = 616;
    pub const APPLICATION_VND_ASTRAEA_SOFTWARE_IOTA: EncodingPrefix = 617;
    pub const APPLICATION_VND_AUDIOGRAPH: EncodingPrefix = 618;
    pub const APPLICATION_VND_AUTOPACKAGE: EncodingPrefix = 619;
    pub const APPLICATION_VND_AVALON_JSON: EncodingPrefix = 620;
    pub const APPLICATION_VND_AVISTAR_XML: EncodingPrefix = 621;
    pub const APPLICATION_VND_BALSAMIQ_BMML_XML: EncodingPrefix = 622;
    pub const APPLICATION_VND_BALSAMIQ_BMPR: EncodingPrefix = 623;
    pub const APPLICATION_VND_BANANA_ACCOUNTING: EncodingPrefix = 624;
    pub const APPLICATION_VND_BBF_USP_ERROR: EncodingPrefix = 625;
    pub const APPLICATION_VND_BBF_USP_MSG: EncodingPrefix = 626;
    pub const APPLICATION_VND_BBF_USP_MSG_JSON: EncodingPrefix = 627;
    pub const APPLICATION_VND_BEKITZUR_STECH_JSON: EncodingPrefix = 628;
    pub const APPLICATION_VND_BELIGHTSOFT_LHZD_ZIP: EncodingPrefix = 629;
    pub const APPLICATION_VND_BELIGHTSOFT_LHZL_ZIP: EncodingPrefix = 630;
    pub const APPLICATION_VND_BINT_MED_CONTENT: EncodingPrefix = 631;
    pub const APPLICATION_VND_BIOPAX_RDF_XML: EncodingPrefix = 632;
    pub const APPLICATION_VND_BLINK_IDB_VALUE_WRAPPER: EncodingPrefix = 633;
    pub const APPLICATION_VND_BLUEICE_MULTIPASS: EncodingPrefix = 634;
    pub const APPLICATION_VND_BLUETOOTH_EP_OOB: EncodingPrefix = 635;
    pub const APPLICATION_VND_BLUETOOTH_LE_OOB: EncodingPrefix = 636;
    pub const APPLICATION_VND_BMI: EncodingPrefix = 637;
    pub const APPLICATION_VND_BPF: EncodingPrefix = 638;
    pub const APPLICATION_VND_BPF3: EncodingPrefix = 639;
    pub const APPLICATION_VND_BUSINESSOBJECTS: EncodingPrefix = 640;
    pub const APPLICATION_VND_BYU_UAPI_JSON: EncodingPrefix = 641;
    pub const APPLICATION_VND_BZIP3: EncodingPrefix = 642;
    pub const APPLICATION_VND_CAB_JSCRIPT: EncodingPrefix = 643;
    pub const APPLICATION_VND_CANON_CPDL: EncodingPrefix = 644;
    pub const APPLICATION_VND_CANON_LIPS: EncodingPrefix = 645;
    pub const APPLICATION_VND_CAPASYSTEMS_PG_JSON: EncodingPrefix = 646;
    pub const APPLICATION_VND_CENDIO_THINLINC_CLIENTCONF: EncodingPrefix = 647;
    pub const APPLICATION_VND_CENTURY_SYSTEMS_TCP_STREAM: EncodingPrefix = 648;
    pub const APPLICATION_VND_CHEMDRAW_XML: EncodingPrefix = 649;
    pub const APPLICATION_VND_CHESS_PGN: EncodingPrefix = 650;
    pub const APPLICATION_VND_CHIPNUTS_KARAOKE_MMD: EncodingPrefix = 651;
    pub const APPLICATION_VND_CIEDI: EncodingPrefix = 652;
    pub const APPLICATION_VND_CINDERELLA: EncodingPrefix = 653;
    pub const APPLICATION_VND_CIRPACK_ISDN_EXT: EncodingPrefix = 654;
    pub const APPLICATION_VND_CITATIONSTYLES_STYLE_XML: EncodingPrefix = 655;
    pub const APPLICATION_VND_CLAYMORE: EncodingPrefix = 656;
    pub const APPLICATION_VND_CLOANTO_RP9: EncodingPrefix = 657;
    pub const APPLICATION_VND_CLONK_C4GROUP: EncodingPrefix = 658;
    pub const APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG: EncodingPrefix = 659;
    pub const APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG_PKG: EncodingPrefix = 660;
    pub const APPLICATION_VND_CNCF_HELM_CHART_CONTENT_V1_TAR_GZIP: EncodingPrefix = 661;
    pub const APPLICATION_VND_CNCF_HELM_CHART_PROVENANCE_V1_PROV: EncodingPrefix = 662;
    pub const APPLICATION_VND_CNCF_HELM_CONFIG_V1_JSON: EncodingPrefix = 663;
    pub const APPLICATION_VND_COFFEESCRIPT: EncodingPrefix = 664;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT: EncodingPrefix = 665;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT_TEMPLATE: EncodingPrefix = 666;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION: EncodingPrefix = 667;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION_TEMPLATE: EncodingPrefix = 668;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET: EncodingPrefix = 669;
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET_TEMPLATE: EncodingPrefix = 670;
    pub const APPLICATION_VND_COLLECTION_JSON: EncodingPrefix = 671;
    pub const APPLICATION_VND_COLLECTION_DOC_JSON: EncodingPrefix = 672;
    pub const APPLICATION_VND_COLLECTION_NEXT_JSON: EncodingPrefix = 673;
    pub const APPLICATION_VND_COMICBOOK_ZIP: EncodingPrefix = 674;
    pub const APPLICATION_VND_COMICBOOK_RAR: EncodingPrefix = 675;
    pub const APPLICATION_VND_COMMERCE_BATTELLE: EncodingPrefix = 676;
    pub const APPLICATION_VND_COMMONSPACE: EncodingPrefix = 677;
    pub const APPLICATION_VND_CONTACT_CMSG: EncodingPrefix = 678;
    pub const APPLICATION_VND_COREOS_IGNITION_JSON: EncodingPrefix = 679;
    pub const APPLICATION_VND_COSMOCALLER: EncodingPrefix = 680;
    pub const APPLICATION_VND_CRICK_CLICKER: EncodingPrefix = 681;
    pub const APPLICATION_VND_CRICK_CLICKER_KEYBOARD: EncodingPrefix = 682;
    pub const APPLICATION_VND_CRICK_CLICKER_PALETTE: EncodingPrefix = 683;
    pub const APPLICATION_VND_CRICK_CLICKER_TEMPLATE: EncodingPrefix = 684;
    pub const APPLICATION_VND_CRICK_CLICKER_WORDBANK: EncodingPrefix = 685;
    pub const APPLICATION_VND_CRITICALTOOLS_WBS_XML: EncodingPrefix = 686;
    pub const APPLICATION_VND_CRYPTII_PIPE_JSON: EncodingPrefix = 687;
    pub const APPLICATION_VND_CRYPTO_SHADE_FILE: EncodingPrefix = 688;
    pub const APPLICATION_VND_CRYPTOMATOR_ENCRYPTED: EncodingPrefix = 689;
    pub const APPLICATION_VND_CRYPTOMATOR_VAULT: EncodingPrefix = 690;
    pub const APPLICATION_VND_CTC_POSML: EncodingPrefix = 691;
    pub const APPLICATION_VND_CTCT_WS_XML: EncodingPrefix = 692;
    pub const APPLICATION_VND_CUPS_PDF: EncodingPrefix = 693;
    pub const APPLICATION_VND_CUPS_POSTSCRIPT: EncodingPrefix = 694;
    pub const APPLICATION_VND_CUPS_PPD: EncodingPrefix = 695;
    pub const APPLICATION_VND_CUPS_RASTER: EncodingPrefix = 696;
    pub const APPLICATION_VND_CUPS_RAW: EncodingPrefix = 697;
    pub const APPLICATION_VND_CURL: EncodingPrefix = 698;
    pub const APPLICATION_VND_CYAN_DEAN_ROOT_XML: EncodingPrefix = 699;
    pub const APPLICATION_VND_CYBANK: EncodingPrefix = 700;
    pub const APPLICATION_VND_CYCLONEDX_JSON: EncodingPrefix = 701;
    pub const APPLICATION_VND_CYCLONEDX_XML: EncodingPrefix = 702;
    pub const APPLICATION_VND_D2L_COURSEPACKAGE1P0_ZIP: EncodingPrefix = 703;
    pub const APPLICATION_VND_D3M_DATASET: EncodingPrefix = 704;
    pub const APPLICATION_VND_D3M_PROBLEM: EncodingPrefix = 705;
    pub const APPLICATION_VND_DART: EncodingPrefix = 706;
    pub const APPLICATION_VND_DATA_VISION_RDZ: EncodingPrefix = 707;
    pub const APPLICATION_VND_DATALOG: EncodingPrefix = 708;
    pub const APPLICATION_VND_DATAPACKAGE_JSON: EncodingPrefix = 709;
    pub const APPLICATION_VND_DATARESOURCE_JSON: EncodingPrefix = 710;
    pub const APPLICATION_VND_DBF: EncodingPrefix = 711;
    pub const APPLICATION_VND_DEBIAN_BINARY_PACKAGE: EncodingPrefix = 712;
    pub const APPLICATION_VND_DECE_DATA: EncodingPrefix = 713;
    pub const APPLICATION_VND_DECE_TTML_XML: EncodingPrefix = 714;
    pub const APPLICATION_VND_DECE_UNSPECIFIED: EncodingPrefix = 715;
    pub const APPLICATION_VND_DECE_ZIP: EncodingPrefix = 716;
    pub const APPLICATION_VND_DENOVO_FCSELAYOUT_LINK: EncodingPrefix = 717;
    pub const APPLICATION_VND_DESMUME_MOVIE: EncodingPrefix = 718;
    pub const APPLICATION_VND_DIR_BI_PLATE_DL_NOSUFFIX: EncodingPrefix = 719;
    pub const APPLICATION_VND_DM_DELEGATION_XML: EncodingPrefix = 720;
    pub const APPLICATION_VND_DNA: EncodingPrefix = 721;
    pub const APPLICATION_VND_DOCUMENT_JSON: EncodingPrefix = 722;
    pub const APPLICATION_VND_DOLBY_MOBILE_1: EncodingPrefix = 723;
    pub const APPLICATION_VND_DOLBY_MOBILE_2: EncodingPrefix = 724;
    pub const APPLICATION_VND_DOREMIR_SCORECLOUD_BINARY_DOCUMENT: EncodingPrefix = 725;
    pub const APPLICATION_VND_DPGRAPH: EncodingPrefix = 726;
    pub const APPLICATION_VND_DREAMFACTORY: EncodingPrefix = 727;
    pub const APPLICATION_VND_DRIVE_JSON: EncodingPrefix = 728;
    pub const APPLICATION_VND_DTG_LOCAL: EncodingPrefix = 729;
    pub const APPLICATION_VND_DTG_LOCAL_FLASH: EncodingPrefix = 730;
    pub const APPLICATION_VND_DTG_LOCAL_HTML: EncodingPrefix = 731;
    pub const APPLICATION_VND_DVB_AIT: EncodingPrefix = 732;
    pub const APPLICATION_VND_DVB_DVBISL_XML: EncodingPrefix = 733;
    pub const APPLICATION_VND_DVB_DVBJ: EncodingPrefix = 734;
    pub const APPLICATION_VND_DVB_ESGCONTAINER: EncodingPrefix = 735;
    pub const APPLICATION_VND_DVB_IPDCDFTNOTIFACCESS: EncodingPrefix = 736;
    pub const APPLICATION_VND_DVB_IPDCESGACCESS: EncodingPrefix = 737;
    pub const APPLICATION_VND_DVB_IPDCESGACCESS2: EncodingPrefix = 738;
    pub const APPLICATION_VND_DVB_IPDCESGPDD: EncodingPrefix = 739;
    pub const APPLICATION_VND_DVB_IPDCROAMING: EncodingPrefix = 740;
    pub const APPLICATION_VND_DVB_IPTV_ALFEC_BASE: EncodingPrefix = 741;
    pub const APPLICATION_VND_DVB_IPTV_ALFEC_ENHANCEMENT: EncodingPrefix = 742;
    pub const APPLICATION_VND_DVB_NOTIF_AGGREGATE_ROOT_XML: EncodingPrefix = 743;
    pub const APPLICATION_VND_DVB_NOTIF_CONTAINER_XML: EncodingPrefix = 744;
    pub const APPLICATION_VND_DVB_NOTIF_GENERIC_XML: EncodingPrefix = 745;
    pub const APPLICATION_VND_DVB_NOTIF_IA_MSGLIST_XML: EncodingPrefix = 746;
    pub const APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_REQUEST_XML: EncodingPrefix = 747;
    pub const APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_RESPONSE_XML: EncodingPrefix = 748;
    pub const APPLICATION_VND_DVB_NOTIF_INIT_XML: EncodingPrefix = 749;
    pub const APPLICATION_VND_DVB_PFR: EncodingPrefix = 750;
    pub const APPLICATION_VND_DVB_SERVICE: EncodingPrefix = 751;
    pub const APPLICATION_VND_DXR: EncodingPrefix = 752;
    pub const APPLICATION_VND_DYNAGEO: EncodingPrefix = 753;
    pub const APPLICATION_VND_DZR: EncodingPrefix = 754;
    pub const APPLICATION_VND_EASYKARAOKE_CDGDOWNLOAD: EncodingPrefix = 755;
    pub const APPLICATION_VND_ECDIS_UPDATE: EncodingPrefix = 756;
    pub const APPLICATION_VND_ECIP_RLP: EncodingPrefix = 757;
    pub const APPLICATION_VND_ECLIPSE_DITTO_JSON: EncodingPrefix = 758;
    pub const APPLICATION_VND_ECOWIN_CHART: EncodingPrefix = 759;
    pub const APPLICATION_VND_ECOWIN_FILEREQUEST: EncodingPrefix = 760;
    pub const APPLICATION_VND_ECOWIN_FILEUPDATE: EncodingPrefix = 761;
    pub const APPLICATION_VND_ECOWIN_SERIES: EncodingPrefix = 762;
    pub const APPLICATION_VND_ECOWIN_SERIESREQUEST: EncodingPrefix = 763;
    pub const APPLICATION_VND_ECOWIN_SERIESUPDATE: EncodingPrefix = 764;
    pub const APPLICATION_VND_EFI_IMG: EncodingPrefix = 765;
    pub const APPLICATION_VND_EFI_ISO: EncodingPrefix = 766;
    pub const APPLICATION_VND_ELN_ZIP: EncodingPrefix = 767;
    pub const APPLICATION_VND_EMCLIENT_ACCESSREQUEST_XML: EncodingPrefix = 768;
    pub const APPLICATION_VND_ENLIVEN: EncodingPrefix = 769;
    pub const APPLICATION_VND_ENPHASE_ENVOY: EncodingPrefix = 770;
    pub const APPLICATION_VND_EPRINTS_DATA_XML: EncodingPrefix = 771;
    pub const APPLICATION_VND_EPSON_ESF: EncodingPrefix = 772;
    pub const APPLICATION_VND_EPSON_MSF: EncodingPrefix = 773;
    pub const APPLICATION_VND_EPSON_QUICKANIME: EncodingPrefix = 774;
    pub const APPLICATION_VND_EPSON_SALT: EncodingPrefix = 775;
    pub const APPLICATION_VND_EPSON_SSF: EncodingPrefix = 776;
    pub const APPLICATION_VND_ERICSSON_QUICKCALL: EncodingPrefix = 777;
    pub const APPLICATION_VND_EROFS: EncodingPrefix = 778;
    pub const APPLICATION_VND_ESPASS_ESPASS_ZIP: EncodingPrefix = 779;
    pub const APPLICATION_VND_ESZIGNO3_XML: EncodingPrefix = 780;
    pub const APPLICATION_VND_ETSI_AOC_XML: EncodingPrefix = 781;
    pub const APPLICATION_VND_ETSI_ASIC_E_ZIP: EncodingPrefix = 782;
    pub const APPLICATION_VND_ETSI_ASIC_S_ZIP: EncodingPrefix = 783;
    pub const APPLICATION_VND_ETSI_CUG_XML: EncodingPrefix = 784;
    pub const APPLICATION_VND_ETSI_IPTVCOMMAND_XML: EncodingPrefix = 785;
    pub const APPLICATION_VND_ETSI_IPTVDISCOVERY_XML: EncodingPrefix = 786;
    pub const APPLICATION_VND_ETSI_IPTVPROFILE_XML: EncodingPrefix = 787;
    pub const APPLICATION_VND_ETSI_IPTVSAD_BC_XML: EncodingPrefix = 788;
    pub const APPLICATION_VND_ETSI_IPTVSAD_COD_XML: EncodingPrefix = 789;
    pub const APPLICATION_VND_ETSI_IPTVSAD_NPVR_XML: EncodingPrefix = 790;
    pub const APPLICATION_VND_ETSI_IPTVSERVICE_XML: EncodingPrefix = 791;
    pub const APPLICATION_VND_ETSI_IPTVSYNC_XML: EncodingPrefix = 792;
    pub const APPLICATION_VND_ETSI_IPTVUEPROFILE_XML: EncodingPrefix = 793;
    pub const APPLICATION_VND_ETSI_MCID_XML: EncodingPrefix = 794;
    pub const APPLICATION_VND_ETSI_MHEG5: EncodingPrefix = 795;
    pub const APPLICATION_VND_ETSI_OVERLOAD_CONTROL_POLICY_DATASET_XML: EncodingPrefix = 796;
    pub const APPLICATION_VND_ETSI_PSTN_XML: EncodingPrefix = 797;
    pub const APPLICATION_VND_ETSI_SCI_XML: EncodingPrefix = 798;
    pub const APPLICATION_VND_ETSI_SIMSERVS_XML: EncodingPrefix = 799;
    pub const APPLICATION_VND_ETSI_TIMESTAMP_TOKEN: EncodingPrefix = 800;
    pub const APPLICATION_VND_ETSI_TSL_XML: EncodingPrefix = 801;
    pub const APPLICATION_VND_ETSI_TSL_DER: EncodingPrefix = 802;
    pub const APPLICATION_VND_EU_KASPARIAN_CAR_JSON: EncodingPrefix = 803;
    pub const APPLICATION_VND_EUDORA_DATA: EncodingPrefix = 804;
    pub const APPLICATION_VND_EVOLV_ECIG_PROFILE: EncodingPrefix = 805;
    pub const APPLICATION_VND_EVOLV_ECIG_SETTINGS: EncodingPrefix = 806;
    pub const APPLICATION_VND_EVOLV_ECIG_THEME: EncodingPrefix = 807;
    pub const APPLICATION_VND_EXSTREAM_EMPOWER_ZIP: EncodingPrefix = 808;
    pub const APPLICATION_VND_EXSTREAM_PACKAGE: EncodingPrefix = 809;
    pub const APPLICATION_VND_EZPIX_ALBUM: EncodingPrefix = 810;
    pub const APPLICATION_VND_EZPIX_PACKAGE: EncodingPrefix = 811;
    pub const APPLICATION_VND_F_SECURE_MOBILE: EncodingPrefix = 812;
    pub const APPLICATION_VND_FAMILYSEARCH_GEDCOM_ZIP: EncodingPrefix = 813;
    pub const APPLICATION_VND_FASTCOPY_DISK_IMAGE: EncodingPrefix = 814;
    pub const APPLICATION_VND_FDSN_MSEED: EncodingPrefix = 815;
    pub const APPLICATION_VND_FDSN_SEED: EncodingPrefix = 816;
    pub const APPLICATION_VND_FFSNS: EncodingPrefix = 817;
    pub const APPLICATION_VND_FICLAB_FLB_ZIP: EncodingPrefix = 818;
    pub const APPLICATION_VND_FILMIT_ZFC: EncodingPrefix = 819;
    pub const APPLICATION_VND_FINTS: EncodingPrefix = 820;
    pub const APPLICATION_VND_FIREMONKEYS_CLOUDCELL: EncodingPrefix = 821;
    pub const APPLICATION_VND_FLUXTIME_CLIP: EncodingPrefix = 822;
    pub const APPLICATION_VND_FONT_FONTFORGE_SFD: EncodingPrefix = 823;
    pub const APPLICATION_VND_FRAMEMAKER: EncodingPrefix = 824;
    pub const APPLICATION_VND_FREELOG_COMIC: EncodingPrefix = 825;
    pub const APPLICATION_VND_FROGANS_FNC: EncodingPrefix = 826;
    pub const APPLICATION_VND_FROGANS_LTF: EncodingPrefix = 827;
    pub const APPLICATION_VND_FSC_WEBLAUNCH: EncodingPrefix = 828;
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS: EncodingPrefix = 829;
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_BINDER: EncodingPrefix = 830;
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_CONTAINER: EncodingPrefix = 831;
    pub const APPLICATION_VND_FUJIFILM_FB_JFI_XML: EncodingPrefix = 832;
    pub const APPLICATION_VND_FUJITSU_OASYS: EncodingPrefix = 833;
    pub const APPLICATION_VND_FUJITSU_OASYS2: EncodingPrefix = 834;
    pub const APPLICATION_VND_FUJITSU_OASYS3: EncodingPrefix = 835;
    pub const APPLICATION_VND_FUJITSU_OASYSGP: EncodingPrefix = 836;
    pub const APPLICATION_VND_FUJITSU_OASYSPRS: EncodingPrefix = 837;
    pub const APPLICATION_VND_FUJIXEROX_ART_EX: EncodingPrefix = 838;
    pub const APPLICATION_VND_FUJIXEROX_ART4: EncodingPrefix = 839;
    pub const APPLICATION_VND_FUJIXEROX_HBPL: EncodingPrefix = 840;
    pub const APPLICATION_VND_FUJIXEROX_DDD: EncodingPrefix = 841;
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS: EncodingPrefix = 842;
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS_BINDER: EncodingPrefix = 843;
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS_CONTAINER: EncodingPrefix = 844;
    pub const APPLICATION_VND_FUT_MISNET: EncodingPrefix = 845;
    pub const APPLICATION_VND_FUTOIN_CBOR: EncodingPrefix = 846;
    pub const APPLICATION_VND_FUTOIN_JSON: EncodingPrefix = 847;
    pub const APPLICATION_VND_FUZZYSHEET: EncodingPrefix = 848;
    pub const APPLICATION_VND_GENOMATIX_TUXEDO: EncodingPrefix = 849;
    pub const APPLICATION_VND_GENOZIP: EncodingPrefix = 850;
    pub const APPLICATION_VND_GENTICS_GRD_JSON: EncodingPrefix = 851;
    pub const APPLICATION_VND_GENTOO_CATMETADATA_XML: EncodingPrefix = 852;
    pub const APPLICATION_VND_GENTOO_EBUILD: EncodingPrefix = 853;
    pub const APPLICATION_VND_GENTOO_ECLASS: EncodingPrefix = 854;
    pub const APPLICATION_VND_GENTOO_GPKG: EncodingPrefix = 855;
    pub const APPLICATION_VND_GENTOO_MANIFEST: EncodingPrefix = 856;
    pub const APPLICATION_VND_GENTOO_PKGMETADATA_XML: EncodingPrefix = 857;
    pub const APPLICATION_VND_GENTOO_XPAK: EncodingPrefix = 858;
    pub const APPLICATION_VND_GEO_JSON: EncodingPrefix = 859;
    pub const APPLICATION_VND_GEOCUBE_XML: EncodingPrefix = 860;
    pub const APPLICATION_VND_GEOGEBRA_FILE: EncodingPrefix = 861;
    pub const APPLICATION_VND_GEOGEBRA_SLIDES: EncodingPrefix = 862;
    pub const APPLICATION_VND_GEOGEBRA_TOOL: EncodingPrefix = 863;
    pub const APPLICATION_VND_GEOMETRY_EXPLORER: EncodingPrefix = 864;
    pub const APPLICATION_VND_GEONEXT: EncodingPrefix = 865;
    pub const APPLICATION_VND_GEOPLAN: EncodingPrefix = 866;
    pub const APPLICATION_VND_GEOSPACE: EncodingPrefix = 867;
    pub const APPLICATION_VND_GERBER: EncodingPrefix = 868;
    pub const APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT: EncodingPrefix = 869;
    pub const APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT_RESPONSE: EncodingPrefix = 870;
    pub const APPLICATION_VND_GMX: EncodingPrefix = 871;
    pub const APPLICATION_VND_GNU_TALER_EXCHANGE_JSON: EncodingPrefix = 872;
    pub const APPLICATION_VND_GNU_TALER_MERCHANT_JSON: EncodingPrefix = 873;
    pub const APPLICATION_VND_GOOGLE_EARTH_KML_XML: EncodingPrefix = 874;
    pub const APPLICATION_VND_GOOGLE_EARTH_KMZ: EncodingPrefix = 875;
    pub const APPLICATION_VND_GOV_SK_E_FORM_XML: EncodingPrefix = 876;
    pub const APPLICATION_VND_GOV_SK_E_FORM_ZIP: EncodingPrefix = 877;
    pub const APPLICATION_VND_GOV_SK_XMLDATACONTAINER_XML: EncodingPrefix = 878;
    pub const APPLICATION_VND_GPXSEE_MAP_XML: EncodingPrefix = 879;
    pub const APPLICATION_VND_GRAFEQ: EncodingPrefix = 880;
    pub const APPLICATION_VND_GRIDMP: EncodingPrefix = 881;
    pub const APPLICATION_VND_GROOVE_ACCOUNT: EncodingPrefix = 882;
    pub const APPLICATION_VND_GROOVE_HELP: EncodingPrefix = 883;
    pub const APPLICATION_VND_GROOVE_IDENTITY_MESSAGE: EncodingPrefix = 884;
    pub const APPLICATION_VND_GROOVE_INJECTOR: EncodingPrefix = 885;
    pub const APPLICATION_VND_GROOVE_TOOL_MESSAGE: EncodingPrefix = 886;
    pub const APPLICATION_VND_GROOVE_TOOL_TEMPLATE: EncodingPrefix = 887;
    pub const APPLICATION_VND_GROOVE_VCARD: EncodingPrefix = 888;
    pub const APPLICATION_VND_HAL_JSON: EncodingPrefix = 889;
    pub const APPLICATION_VND_HAL_XML: EncodingPrefix = 890;
    pub const APPLICATION_VND_HBCI: EncodingPrefix = 891;
    pub const APPLICATION_VND_HC_JSON: EncodingPrefix = 892;
    pub const APPLICATION_VND_HCL_BIREPORTS: EncodingPrefix = 893;
    pub const APPLICATION_VND_HDT: EncodingPrefix = 894;
    pub const APPLICATION_VND_HEROKU_JSON: EncodingPrefix = 895;
    pub const APPLICATION_VND_HHE_LESSON_PLAYER: EncodingPrefix = 896;
    pub const APPLICATION_VND_HP_HPGL: EncodingPrefix = 897;
    pub const APPLICATION_VND_HP_PCL: EncodingPrefix = 898;
    pub const APPLICATION_VND_HP_PCLXL: EncodingPrefix = 899;
    pub const APPLICATION_VND_HP_HPID: EncodingPrefix = 900;
    pub const APPLICATION_VND_HP_HPS: EncodingPrefix = 901;
    pub const APPLICATION_VND_HP_JLYT: EncodingPrefix = 902;
    pub const APPLICATION_VND_HSL: EncodingPrefix = 903;
    pub const APPLICATION_VND_HTTPHONE: EncodingPrefix = 904;
    pub const APPLICATION_VND_HYDROSTATIX_SOF_DATA: EncodingPrefix = 905;
    pub const APPLICATION_VND_HYPER_JSON: EncodingPrefix = 906;
    pub const APPLICATION_VND_HYPER_ITEM_JSON: EncodingPrefix = 907;
    pub const APPLICATION_VND_HYPERDRIVE_JSON: EncodingPrefix = 908;
    pub const APPLICATION_VND_HZN_3D_CROSSWORD: EncodingPrefix = 909;
    pub const APPLICATION_VND_IBM_MINIPAY: EncodingPrefix = 910;
    pub const APPLICATION_VND_IBM_AFPLINEDATA: EncodingPrefix = 911;
    pub const APPLICATION_VND_IBM_ELECTRONIC_MEDIA: EncodingPrefix = 912;
    pub const APPLICATION_VND_IBM_MODCAP: EncodingPrefix = 913;
    pub const APPLICATION_VND_IBM_RIGHTS_MANAGEMENT: EncodingPrefix = 914;
    pub const APPLICATION_VND_IBM_SECURE_CONTAINER: EncodingPrefix = 915;
    pub const APPLICATION_VND_ICCPROFILE: EncodingPrefix = 916;
    pub const APPLICATION_VND_IEEE_1905: EncodingPrefix = 917;
    pub const APPLICATION_VND_IGLOADER: EncodingPrefix = 918;
    pub const APPLICATION_VND_IMAGEMETER_FOLDER_ZIP: EncodingPrefix = 919;
    pub const APPLICATION_VND_IMAGEMETER_IMAGE_ZIP: EncodingPrefix = 920;
    pub const APPLICATION_VND_IMMERVISION_IVP: EncodingPrefix = 921;
    pub const APPLICATION_VND_IMMERVISION_IVU: EncodingPrefix = 922;
    pub const APPLICATION_VND_IMS_IMSCCV1P1: EncodingPrefix = 923;
    pub const APPLICATION_VND_IMS_IMSCCV1P2: EncodingPrefix = 924;
    pub const APPLICATION_VND_IMS_IMSCCV1P3: EncodingPrefix = 925;
    pub const APPLICATION_VND_IMS_LIS_V2_RESULT_JSON: EncodingPrefix = 926;
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLCONSUMERPROFILE_JSON: EncodingPrefix = 927;
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_JSON: EncodingPrefix = 928;
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_ID_JSON: EncodingPrefix = 929;
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_JSON: EncodingPrefix = 930;
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_SIMPLE_JSON: EncodingPrefix = 931;
    pub const APPLICATION_VND_INFORMEDCONTROL_RMS_XML: EncodingPrefix = 932;
    pub const APPLICATION_VND_INFORMIX_VISIONARY: EncodingPrefix = 933;
    pub const APPLICATION_VND_INFOTECH_PROJECT: EncodingPrefix = 934;
    pub const APPLICATION_VND_INFOTECH_PROJECT_XML: EncodingPrefix = 935;
    pub const APPLICATION_VND_INNOPATH_WAMP_NOTIFICATION: EncodingPrefix = 936;
    pub const APPLICATION_VND_INSORS_IGM: EncodingPrefix = 937;
    pub const APPLICATION_VND_INTERCON_FORMNET: EncodingPrefix = 938;
    pub const APPLICATION_VND_INTERGEO: EncodingPrefix = 939;
    pub const APPLICATION_VND_INTERTRUST_DIGIBOX: EncodingPrefix = 940;
    pub const APPLICATION_VND_INTERTRUST_NNCP: EncodingPrefix = 941;
    pub const APPLICATION_VND_INTU_QBO: EncodingPrefix = 942;
    pub const APPLICATION_VND_INTU_QFX: EncodingPrefix = 943;
    pub const APPLICATION_VND_IPFS_IPNS_RECORD: EncodingPrefix = 944;
    pub const APPLICATION_VND_IPLD_CAR: EncodingPrefix = 945;
    pub const APPLICATION_VND_IPLD_DAG_CBOR: EncodingPrefix = 946;
    pub const APPLICATION_VND_IPLD_DAG_JSON: EncodingPrefix = 947;
    pub const APPLICATION_VND_IPLD_RAW: EncodingPrefix = 948;
    pub const APPLICATION_VND_IPTC_G2_CATALOGITEM_XML: EncodingPrefix = 949;
    pub const APPLICATION_VND_IPTC_G2_CONCEPTITEM_XML: EncodingPrefix = 950;
    pub const APPLICATION_VND_IPTC_G2_KNOWLEDGEITEM_XML: EncodingPrefix = 951;
    pub const APPLICATION_VND_IPTC_G2_NEWSITEM_XML: EncodingPrefix = 952;
    pub const APPLICATION_VND_IPTC_G2_NEWSMESSAGE_XML: EncodingPrefix = 953;
    pub const APPLICATION_VND_IPTC_G2_PACKAGEITEM_XML: EncodingPrefix = 954;
    pub const APPLICATION_VND_IPTC_G2_PLANNINGITEM_XML: EncodingPrefix = 955;
    pub const APPLICATION_VND_IPUNPLUGGED_RCPROFILE: EncodingPrefix = 956;
    pub const APPLICATION_VND_IREPOSITORY_PACKAGE_XML: EncodingPrefix = 957;
    pub const APPLICATION_VND_IS_XPR: EncodingPrefix = 958;
    pub const APPLICATION_VND_ISAC_FCS: EncodingPrefix = 959;
    pub const APPLICATION_VND_ISO11783_10_ZIP: EncodingPrefix = 960;
    pub const APPLICATION_VND_JAM: EncodingPrefix = 961;
    pub const APPLICATION_VND_JAPANNET_DIRECTORY_SERVICE: EncodingPrefix = 962;
    pub const APPLICATION_VND_JAPANNET_JPNSTORE_WAKEUP: EncodingPrefix = 963;
    pub const APPLICATION_VND_JAPANNET_PAYMENT_WAKEUP: EncodingPrefix = 964;
    pub const APPLICATION_VND_JAPANNET_REGISTRATION: EncodingPrefix = 965;
    pub const APPLICATION_VND_JAPANNET_REGISTRATION_WAKEUP: EncodingPrefix = 966;
    pub const APPLICATION_VND_JAPANNET_SETSTORE_WAKEUP: EncodingPrefix = 967;
    pub const APPLICATION_VND_JAPANNET_VERIFICATION: EncodingPrefix = 968;
    pub const APPLICATION_VND_JAPANNET_VERIFICATION_WAKEUP: EncodingPrefix = 969;
    pub const APPLICATION_VND_JCP_JAVAME_MIDLET_RMS: EncodingPrefix = 970;
    pub const APPLICATION_VND_JISP: EncodingPrefix = 971;
    pub const APPLICATION_VND_JOOST_JODA_ARCHIVE: EncodingPrefix = 972;
    pub const APPLICATION_VND_JSK_ISDN_NGN: EncodingPrefix = 973;
    pub const APPLICATION_VND_KAHOOTZ: EncodingPrefix = 974;
    pub const APPLICATION_VND_KDE_KARBON: EncodingPrefix = 975;
    pub const APPLICATION_VND_KDE_KCHART: EncodingPrefix = 976;
    pub const APPLICATION_VND_KDE_KFORMULA: EncodingPrefix = 977;
    pub const APPLICATION_VND_KDE_KIVIO: EncodingPrefix = 978;
    pub const APPLICATION_VND_KDE_KONTOUR: EncodingPrefix = 979;
    pub const APPLICATION_VND_KDE_KPRESENTER: EncodingPrefix = 980;
    pub const APPLICATION_VND_KDE_KSPREAD: EncodingPrefix = 981;
    pub const APPLICATION_VND_KDE_KWORD: EncodingPrefix = 982;
    pub const APPLICATION_VND_KENAMEAAPP: EncodingPrefix = 983;
    pub const APPLICATION_VND_KIDSPIRATION: EncodingPrefix = 984;
    pub const APPLICATION_VND_KOAN: EncodingPrefix = 985;
    pub const APPLICATION_VND_KODAK_DESCRIPTOR: EncodingPrefix = 986;
    pub const APPLICATION_VND_LAS: EncodingPrefix = 987;
    pub const APPLICATION_VND_LAS_LAS_JSON: EncodingPrefix = 988;
    pub const APPLICATION_VND_LAS_LAS_XML: EncodingPrefix = 989;
    pub const APPLICATION_VND_LASZIP: EncodingPrefix = 990;
    pub const APPLICATION_VND_LDEV_PRODUCTLICENSING: EncodingPrefix = 991;
    pub const APPLICATION_VND_LEAP_JSON: EncodingPrefix = 992;
    pub const APPLICATION_VND_LIBERTY_REQUEST_XML: EncodingPrefix = 993;
    pub const APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_DESKTOP: EncodingPrefix = 994;
    pub const APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_EXCHANGE_XML: EncodingPrefix = 995;
    pub const APPLICATION_VND_LOGIPIPE_CIRCUIT_ZIP: EncodingPrefix = 996;
    pub const APPLICATION_VND_LOOM: EncodingPrefix = 997;
    pub const APPLICATION_VND_LOTUS_1_2_3: EncodingPrefix = 998;
    pub const APPLICATION_VND_LOTUS_APPROACH: EncodingPrefix = 999;
    pub const APPLICATION_VND_LOTUS_FREELANCE: EncodingPrefix = 1000;
    pub const APPLICATION_VND_LOTUS_NOTES: EncodingPrefix = 1001;
    pub const APPLICATION_VND_LOTUS_ORGANIZER: EncodingPrefix = 1002;
    pub const APPLICATION_VND_LOTUS_SCREENCAM: EncodingPrefix = 1003;
    pub const APPLICATION_VND_LOTUS_WORDPRO: EncodingPrefix = 1004;
    pub const APPLICATION_VND_MACPORTS_PORTPKG: EncodingPrefix = 1005;
    pub const APPLICATION_VND_MAPBOX_VECTOR_TILE: EncodingPrefix = 1006;
    pub const APPLICATION_VND_MARLIN_DRM_ACTIONTOKEN_XML: EncodingPrefix = 1007;
    pub const APPLICATION_VND_MARLIN_DRM_CONFTOKEN_XML: EncodingPrefix = 1008;
    pub const APPLICATION_VND_MARLIN_DRM_LICENSE_XML: EncodingPrefix = 1009;
    pub const APPLICATION_VND_MARLIN_DRM_MDCF: EncodingPrefix = 1010;
    pub const APPLICATION_VND_MASON_JSON: EncodingPrefix = 1011;
    pub const APPLICATION_VND_MAXAR_ARCHIVE_3TZ_ZIP: EncodingPrefix = 1012;
    pub const APPLICATION_VND_MAXMIND_MAXMIND_DB: EncodingPrefix = 1013;
    pub const APPLICATION_VND_MCD: EncodingPrefix = 1014;
    pub const APPLICATION_VND_MDL: EncodingPrefix = 1015;
    pub const APPLICATION_VND_MDL_MBSDF: EncodingPrefix = 1016;
    pub const APPLICATION_VND_MEDCALCDATA: EncodingPrefix = 1017;
    pub const APPLICATION_VND_MEDIASTATION_CDKEY: EncodingPrefix = 1018;
    pub const APPLICATION_VND_MEDICALHOLODECK_RECORDXR: EncodingPrefix = 1019;
    pub const APPLICATION_VND_MERIDIAN_SLINGSHOT: EncodingPrefix = 1020;
    pub const APPLICATION_VND_MERMAID: EncodingPrefix = 1021;
    pub const APPLICATION_VND_MFMP: EncodingPrefix = 1022;
    pub const APPLICATION_VND_MICRO_JSON: EncodingPrefix = 1023;
    pub const APPLICATION_VND_MICROGRAFX_FLO: EncodingPrefix = 1024;
    pub const APPLICATION_VND_MICROGRAFX_IGX: EncodingPrefix = 1025;
    pub const APPLICATION_VND_MICROSOFT_PORTABLE_EXECUTABLE: EncodingPrefix = 1026;
    pub const APPLICATION_VND_MICROSOFT_WINDOWS_THUMBNAIL_CACHE: EncodingPrefix = 1027;
    pub const APPLICATION_VND_MIELE_JSON: EncodingPrefix = 1028;
    pub const APPLICATION_VND_MIF: EncodingPrefix = 1029;
    pub const APPLICATION_VND_MINISOFT_HP3000_SAVE: EncodingPrefix = 1030;
    pub const APPLICATION_VND_MITSUBISHI_MISTY_GUARD_TRUSTWEB: EncodingPrefix = 1031;
    pub const APPLICATION_VND_MODL: EncodingPrefix = 1032;
    pub const APPLICATION_VND_MOPHUN_APPLICATION: EncodingPrefix = 1033;
    pub const APPLICATION_VND_MOPHUN_CERTIFICATE: EncodingPrefix = 1034;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE: EncodingPrefix = 1035;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_ADSI: EncodingPrefix = 1036;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_FIS: EncodingPrefix = 1037;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_GOTAP: EncodingPrefix = 1038;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_KMR: EncodingPrefix = 1039;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_TTC: EncodingPrefix = 1040;
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_WEM: EncodingPrefix = 1041;
    pub const APPLICATION_VND_MOTOROLA_IPRM: EncodingPrefix = 1042;
    pub const APPLICATION_VND_MOZILLA_XUL_XML: EncodingPrefix = 1043;
    pub const APPLICATION_VND_MS_3MFDOCUMENT: EncodingPrefix = 1044;
    pub const APPLICATION_VND_MS_PRINTDEVICECAPABILITIES_XML: EncodingPrefix = 1045;
    pub const APPLICATION_VND_MS_PRINTSCHEMATICKET_XML: EncodingPrefix = 1046;
    pub const APPLICATION_VND_MS_ARTGALRY: EncodingPrefix = 1047;
    pub const APPLICATION_VND_MS_ASF: EncodingPrefix = 1048;
    pub const APPLICATION_VND_MS_CAB_COMPRESSED: EncodingPrefix = 1049;
    pub const APPLICATION_VND_MS_EXCEL: EncodingPrefix = 1050;
    pub const APPLICATION_VND_MS_EXCEL_ADDIN_MACROENABLED_12: EncodingPrefix = 1051;
    pub const APPLICATION_VND_MS_EXCEL_SHEET_BINARY_MACROENABLED_12: EncodingPrefix = 1052;
    pub const APPLICATION_VND_MS_EXCEL_SHEET_MACROENABLED_12: EncodingPrefix = 1053;
    pub const APPLICATION_VND_MS_EXCEL_TEMPLATE_MACROENABLED_12: EncodingPrefix = 1054;
    pub const APPLICATION_VND_MS_FONTOBJECT: EncodingPrefix = 1055;
    pub const APPLICATION_VND_MS_HTMLHELP: EncodingPrefix = 1056;
    pub const APPLICATION_VND_MS_IMS: EncodingPrefix = 1057;
    pub const APPLICATION_VND_MS_LRM: EncodingPrefix = 1058;
    pub const APPLICATION_VND_MS_OFFICE_ACTIVEX_XML: EncodingPrefix = 1059;
    pub const APPLICATION_VND_MS_OFFICETHEME: EncodingPrefix = 1060;
    pub const APPLICATION_VND_MS_PLAYREADY_INITIATOR_XML: EncodingPrefix = 1061;
    pub const APPLICATION_VND_MS_POWERPOINT: EncodingPrefix = 1062;
    pub const APPLICATION_VND_MS_POWERPOINT_ADDIN_MACROENABLED_12: EncodingPrefix = 1063;
    pub const APPLICATION_VND_MS_POWERPOINT_PRESENTATION_MACROENABLED_12: EncodingPrefix = 1064;
    pub const APPLICATION_VND_MS_POWERPOINT_SLIDE_MACROENABLED_12: EncodingPrefix = 1065;
    pub const APPLICATION_VND_MS_POWERPOINT_SLIDESHOW_MACROENABLED_12: EncodingPrefix = 1066;
    pub const APPLICATION_VND_MS_POWERPOINT_TEMPLATE_MACROENABLED_12: EncodingPrefix = 1067;
    pub const APPLICATION_VND_MS_PROJECT: EncodingPrefix = 1068;
    pub const APPLICATION_VND_MS_TNEF: EncodingPrefix = 1069;
    pub const APPLICATION_VND_MS_WINDOWS_DEVICEPAIRING: EncodingPrefix = 1070;
    pub const APPLICATION_VND_MS_WINDOWS_NWPRINTING_OOB: EncodingPrefix = 1071;
    pub const APPLICATION_VND_MS_WINDOWS_PRINTERPAIRING: EncodingPrefix = 1072;
    pub const APPLICATION_VND_MS_WINDOWS_WSD_OOB: EncodingPrefix = 1073;
    pub const APPLICATION_VND_MS_WMDRM_LIC_CHLG_REQ: EncodingPrefix = 1074;
    pub const APPLICATION_VND_MS_WMDRM_LIC_RESP: EncodingPrefix = 1075;
    pub const APPLICATION_VND_MS_WMDRM_METER_CHLG_REQ: EncodingPrefix = 1076;
    pub const APPLICATION_VND_MS_WMDRM_METER_RESP: EncodingPrefix = 1077;
    pub const APPLICATION_VND_MS_WORD_DOCUMENT_MACROENABLED_12: EncodingPrefix = 1078;
    pub const APPLICATION_VND_MS_WORD_TEMPLATE_MACROENABLED_12: EncodingPrefix = 1079;
    pub const APPLICATION_VND_MS_WORKS: EncodingPrefix = 1080;
    pub const APPLICATION_VND_MS_WPL: EncodingPrefix = 1081;
    pub const APPLICATION_VND_MS_XPSDOCUMENT: EncodingPrefix = 1082;
    pub const APPLICATION_VND_MSA_DISK_IMAGE: EncodingPrefix = 1083;
    pub const APPLICATION_VND_MSEQ: EncodingPrefix = 1084;
    pub const APPLICATION_VND_MSIGN: EncodingPrefix = 1085;
    pub const APPLICATION_VND_MULTIAD_CREATOR: EncodingPrefix = 1086;
    pub const APPLICATION_VND_MULTIAD_CREATOR_CIF: EncodingPrefix = 1087;
    pub const APPLICATION_VND_MUSIC_NIFF: EncodingPrefix = 1088;
    pub const APPLICATION_VND_MUSICIAN: EncodingPrefix = 1089;
    pub const APPLICATION_VND_MUVEE_STYLE: EncodingPrefix = 1090;
    pub const APPLICATION_VND_MYNFC: EncodingPrefix = 1091;
    pub const APPLICATION_VND_NACAMAR_YBRID_JSON: EncodingPrefix = 1092;
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_CBOR: EncodingPrefix = 1093;
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_JSON: EncodingPrefix = 1094;
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_XML: EncodingPrefix = 1095;
    pub const APPLICATION_VND_NATO_OPENXMLFORMATS_PACKAGE_IEPD_ZIP: EncodingPrefix = 1096;
    pub const APPLICATION_VND_NCD_CONTROL: EncodingPrefix = 1097;
    pub const APPLICATION_VND_NCD_REFERENCE: EncodingPrefix = 1098;
    pub const APPLICATION_VND_NEARST_INV_JSON: EncodingPrefix = 1099;
    pub const APPLICATION_VND_NEBUMIND_LINE: EncodingPrefix = 1100;
    pub const APPLICATION_VND_NERVANA: EncodingPrefix = 1101;
    pub const APPLICATION_VND_NETFPX: EncodingPrefix = 1102;
    pub const APPLICATION_VND_NEUROLANGUAGE_NLU: EncodingPrefix = 1103;
    pub const APPLICATION_VND_NIMN: EncodingPrefix = 1104;
    pub const APPLICATION_VND_NINTENDO_NITRO_ROM: EncodingPrefix = 1105;
    pub const APPLICATION_VND_NINTENDO_SNES_ROM: EncodingPrefix = 1106;
    pub const APPLICATION_VND_NITF: EncodingPrefix = 1107;
    pub const APPLICATION_VND_NOBLENET_DIRECTORY: EncodingPrefix = 1108;
    pub const APPLICATION_VND_NOBLENET_SEALER: EncodingPrefix = 1109;
    pub const APPLICATION_VND_NOBLENET_WEB: EncodingPrefix = 1110;
    pub const APPLICATION_VND_NOKIA_CATALOGS: EncodingPrefix = 1111;
    pub const APPLICATION_VND_NOKIA_CONML_WBXML: EncodingPrefix = 1112;
    pub const APPLICATION_VND_NOKIA_CONML_XML: EncodingPrefix = 1113;
    pub const APPLICATION_VND_NOKIA_ISDS_RADIO_PRESETS: EncodingPrefix = 1114;
    pub const APPLICATION_VND_NOKIA_IPTV_CONFIG_XML: EncodingPrefix = 1115;
    pub const APPLICATION_VND_NOKIA_LANDMARK_WBXML: EncodingPrefix = 1116;
    pub const APPLICATION_VND_NOKIA_LANDMARK_XML: EncodingPrefix = 1117;
    pub const APPLICATION_VND_NOKIA_LANDMARKCOLLECTION_XML: EncodingPrefix = 1118;
    pub const APPLICATION_VND_NOKIA_N_GAGE_AC_XML: EncodingPrefix = 1119;
    pub const APPLICATION_VND_NOKIA_N_GAGE_DATA: EncodingPrefix = 1120;
    pub const APPLICATION_VND_NOKIA_N_GAGE_SYMBIAN_INSTALL: EncodingPrefix = 1121;
    pub const APPLICATION_VND_NOKIA_NCD: EncodingPrefix = 1122;
    pub const APPLICATION_VND_NOKIA_PCD_WBXML: EncodingPrefix = 1123;
    pub const APPLICATION_VND_NOKIA_PCD_XML: EncodingPrefix = 1124;
    pub const APPLICATION_VND_NOKIA_RADIO_PRESET: EncodingPrefix = 1125;
    pub const APPLICATION_VND_NOKIA_RADIO_PRESETS: EncodingPrefix = 1126;
    pub const APPLICATION_VND_NOVADIGM_EDM: EncodingPrefix = 1127;
    pub const APPLICATION_VND_NOVADIGM_EDX: EncodingPrefix = 1128;
    pub const APPLICATION_VND_NOVADIGM_EXT: EncodingPrefix = 1129;
    pub const APPLICATION_VND_NTT_LOCAL_CONTENT_SHARE: EncodingPrefix = 1130;
    pub const APPLICATION_VND_NTT_LOCAL_FILE_TRANSFER: EncodingPrefix = 1131;
    pub const APPLICATION_VND_NTT_LOCAL_OGW_REMOTE_ACCESS: EncodingPrefix = 1132;
    pub const APPLICATION_VND_NTT_LOCAL_SIP_TA_REMOTE: EncodingPrefix = 1133;
    pub const APPLICATION_VND_NTT_LOCAL_SIP_TA_TCP_STREAM: EncodingPrefix = 1134;
    pub const APPLICATION_VND_OAI_WORKFLOWS: EncodingPrefix = 1135;
    pub const APPLICATION_VND_OAI_WORKFLOWS_JSON: EncodingPrefix = 1136;
    pub const APPLICATION_VND_OAI_WORKFLOWS_YAML: EncodingPrefix = 1137;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_BASE: EncodingPrefix = 1138;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_CHART: EncodingPrefix = 1139;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_CHART_TEMPLATE: EncodingPrefix = 1140;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_DATABASE: EncodingPrefix = 1141;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA: EncodingPrefix = 1142;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA_TEMPLATE: EncodingPrefix = 1143;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS: EncodingPrefix = 1144;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS_TEMPLATE: EncodingPrefix = 1145;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE: EncodingPrefix = 1146;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE_TEMPLATE: EncodingPrefix = 1147;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION: EncodingPrefix = 1148;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION_TEMPLATE: EncodingPrefix = 1149;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET: EncodingPrefix = 1150;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET_TEMPLATE: EncodingPrefix = 1151;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT: EncodingPrefix = 1152;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER: EncodingPrefix = 1153;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER_TEMPLATE: EncodingPrefix = 1154;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_TEMPLATE: EncodingPrefix = 1155;
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_WEB: EncodingPrefix = 1156;
    pub const APPLICATION_VND_OBN: EncodingPrefix = 1157;
    pub const APPLICATION_VND_OCF_CBOR: EncodingPrefix = 1158;
    pub const APPLICATION_VND_OCI_IMAGE_MANIFEST_V1_JSON: EncodingPrefix = 1159;
    pub const APPLICATION_VND_OFTN_L10N_JSON: EncodingPrefix = 1160;
    pub const APPLICATION_VND_OIPF_CONTENTACCESSDOWNLOAD_XML: EncodingPrefix = 1161;
    pub const APPLICATION_VND_OIPF_CONTENTACCESSSTREAMING_XML: EncodingPrefix = 1162;
    pub const APPLICATION_VND_OIPF_CSPG_HEXBINARY: EncodingPrefix = 1163;
    pub const APPLICATION_VND_OIPF_DAE_SVG_XML: EncodingPrefix = 1164;
    pub const APPLICATION_VND_OIPF_DAE_XHTML_XML: EncodingPrefix = 1165;
    pub const APPLICATION_VND_OIPF_MIPPVCONTROLMESSAGE_XML: EncodingPrefix = 1166;
    pub const APPLICATION_VND_OIPF_PAE_GEM: EncodingPrefix = 1167;
    pub const APPLICATION_VND_OIPF_SPDISCOVERY_XML: EncodingPrefix = 1168;
    pub const APPLICATION_VND_OIPF_SPDLIST_XML: EncodingPrefix = 1169;
    pub const APPLICATION_VND_OIPF_UEPROFILE_XML: EncodingPrefix = 1170;
    pub const APPLICATION_VND_OIPF_USERPROFILE_XML: EncodingPrefix = 1171;
    pub const APPLICATION_VND_OLPC_SUGAR: EncodingPrefix = 1172;
    pub const APPLICATION_VND_OMA_SCWS_CONFIG: EncodingPrefix = 1173;
    pub const APPLICATION_VND_OMA_SCWS_HTTP_REQUEST: EncodingPrefix = 1174;
    pub const APPLICATION_VND_OMA_SCWS_HTTP_RESPONSE: EncodingPrefix = 1175;
    pub const APPLICATION_VND_OMA_BCAST_ASSOCIATED_PROCEDURE_PARAMETER_XML: EncodingPrefix = 1176;
    pub const APPLICATION_VND_OMA_BCAST_DRM_TRIGGER_XML: EncodingPrefix = 1177;
    pub const APPLICATION_VND_OMA_BCAST_IMD_XML: EncodingPrefix = 1178;
    pub const APPLICATION_VND_OMA_BCAST_LTKM: EncodingPrefix = 1179;
    pub const APPLICATION_VND_OMA_BCAST_NOTIFICATION_XML: EncodingPrefix = 1180;
    pub const APPLICATION_VND_OMA_BCAST_PROVISIONINGTRIGGER: EncodingPrefix = 1181;
    pub const APPLICATION_VND_OMA_BCAST_SGBOOT: EncodingPrefix = 1182;
    pub const APPLICATION_VND_OMA_BCAST_SGDD_XML: EncodingPrefix = 1183;
    pub const APPLICATION_VND_OMA_BCAST_SGDU: EncodingPrefix = 1184;
    pub const APPLICATION_VND_OMA_BCAST_SIMPLE_SYMBOL_CONTAINER: EncodingPrefix = 1185;
    pub const APPLICATION_VND_OMA_BCAST_SMARTCARD_TRIGGER_XML: EncodingPrefix = 1186;
    pub const APPLICATION_VND_OMA_BCAST_SPROV_XML: EncodingPrefix = 1187;
    pub const APPLICATION_VND_OMA_BCAST_STKM: EncodingPrefix = 1188;
    pub const APPLICATION_VND_OMA_CAB_ADDRESS_BOOK_XML: EncodingPrefix = 1189;
    pub const APPLICATION_VND_OMA_CAB_FEATURE_HANDLER_XML: EncodingPrefix = 1190;
    pub const APPLICATION_VND_OMA_CAB_PCC_XML: EncodingPrefix = 1191;
    pub const APPLICATION_VND_OMA_CAB_SUBS_INVITE_XML: EncodingPrefix = 1192;
    pub const APPLICATION_VND_OMA_CAB_USER_PREFS_XML: EncodingPrefix = 1193;
    pub const APPLICATION_VND_OMA_DCD: EncodingPrefix = 1194;
    pub const APPLICATION_VND_OMA_DCDC: EncodingPrefix = 1195;
    pub const APPLICATION_VND_OMA_DD2_XML: EncodingPrefix = 1196;
    pub const APPLICATION_VND_OMA_DRM_RISD_XML: EncodingPrefix = 1197;
    pub const APPLICATION_VND_OMA_GROUP_USAGE_LIST_XML: EncodingPrefix = 1198;
    pub const APPLICATION_VND_OMA_LWM2M_CBOR: EncodingPrefix = 1199;
    pub const APPLICATION_VND_OMA_LWM2M_JSON: EncodingPrefix = 1200;
    pub const APPLICATION_VND_OMA_LWM2M_TLV: EncodingPrefix = 1201;
    pub const APPLICATION_VND_OMA_PAL_XML: EncodingPrefix = 1202;
    pub const APPLICATION_VND_OMA_POC_DETAILED_PROGRESS_REPORT_XML: EncodingPrefix = 1203;
    pub const APPLICATION_VND_OMA_POC_FINAL_REPORT_XML: EncodingPrefix = 1204;
    pub const APPLICATION_VND_OMA_POC_GROUPS_XML: EncodingPrefix = 1205;
    pub const APPLICATION_VND_OMA_POC_INVOCATION_DESCRIPTOR_XML: EncodingPrefix = 1206;
    pub const APPLICATION_VND_OMA_POC_OPTIMIZED_PROGRESS_REPORT_XML: EncodingPrefix = 1207;
    pub const APPLICATION_VND_OMA_PUSH: EncodingPrefix = 1208;
    pub const APPLICATION_VND_OMA_SCIDM_MESSAGES_XML: EncodingPrefix = 1209;
    pub const APPLICATION_VND_OMA_XCAP_DIRECTORY_XML: EncodingPrefix = 1210;
    pub const APPLICATION_VND_OMADS_EMAIL_XML: EncodingPrefix = 1211;
    pub const APPLICATION_VND_OMADS_FILE_XML: EncodingPrefix = 1212;
    pub const APPLICATION_VND_OMADS_FOLDER_XML: EncodingPrefix = 1213;
    pub const APPLICATION_VND_OMALOC_SUPL_INIT: EncodingPrefix = 1214;
    pub const APPLICATION_VND_ONEPAGER: EncodingPrefix = 1215;
    pub const APPLICATION_VND_ONEPAGERTAMP: EncodingPrefix = 1216;
    pub const APPLICATION_VND_ONEPAGERTAMX: EncodingPrefix = 1217;
    pub const APPLICATION_VND_ONEPAGERTAT: EncodingPrefix = 1218;
    pub const APPLICATION_VND_ONEPAGERTATP: EncodingPrefix = 1219;
    pub const APPLICATION_VND_ONEPAGERTATX: EncodingPrefix = 1220;
    pub const APPLICATION_VND_ONVIF_METADATA: EncodingPrefix = 1221;
    pub const APPLICATION_VND_OPENBLOX_GAME_XML: EncodingPrefix = 1222;
    pub const APPLICATION_VND_OPENBLOX_GAME_BINARY: EncodingPrefix = 1223;
    pub const APPLICATION_VND_OPENEYE_OEB: EncodingPrefix = 1224;
    pub const APPLICATION_VND_OPENSTREETMAP_DATA_XML: EncodingPrefix = 1225;
    pub const APPLICATION_VND_OPENTIMESTAMPS_OTS: EncodingPrefix = 1226;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOM_PROPERTIES_XML: EncodingPrefix =
        1227;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOMXMLPROPERTIES_XML:
        EncodingPrefix = 1228;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWING_XML: EncodingPrefix = 1229;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHART_XML: EncodingPrefix =
        1230;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHARTSHAPES_XML:
        EncodingPrefix = 1231;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMCOLORS_XML:
        EncodingPrefix = 1232;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMDATA_XML:
        EncodingPrefix = 1233;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMLAYOUT_XML:
        EncodingPrefix = 1234;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMSTYLE_XML:
        EncodingPrefix = 1235;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_EXTENDED_PROPERTIES_XML:
        EncodingPrefix = 1236;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTAUTHORS_XML:
        EncodingPrefix = 1237;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTS_XML:
        EncodingPrefix = 1238;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_HANDOUTMASTER_XML:
        EncodingPrefix = 1239;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESMASTER_XML:
        EncodingPrefix = 1240;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESSLIDE_XML:
        EncodingPrefix = 1241;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESPROPS_XML:
        EncodingPrefix = 1242;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION:
        EncodingPrefix = 1243;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION_MAIN_XML:
        EncodingPrefix = 1244;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE: EncodingPrefix =
        1245;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE_XML:
        EncodingPrefix = 1246;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDELAYOUT_XML:
        EncodingPrefix = 1247;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEMASTER_XML:
        EncodingPrefix = 1248;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEUPDATEINFO_XML:
        EncodingPrefix = 1249;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW:
        EncodingPrefix = 1250;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW_MAIN_XML:
        EncodingPrefix = 1251;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TABLESTYLES_XML:
        EncodingPrefix = 1252;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TAGS_XML:
        EncodingPrefix = 1253;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE:
        EncodingPrefix = 1254;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE_MAIN_XML:
        EncodingPrefix = 1255;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_VIEWPROPS_XML:
        EncodingPrefix = 1256;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CALCCHAIN_XML:
        EncodingPrefix = 1257;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CHARTSHEET_XML:
        EncodingPrefix = 1258;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_COMMENTS_XML:
        EncodingPrefix = 1259;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CONNECTIONS_XML:
        EncodingPrefix = 1260;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_DIALOGSHEET_XML:
        EncodingPrefix = 1261;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_EXTERNALLINK_XML:
        EncodingPrefix = 1262;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHEDEFINITION_XML: EncodingPrefix = 1263;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHERECORDS_XML:
        EncodingPrefix = 1264;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTTABLE_XML:
        EncodingPrefix = 1265;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_QUERYTABLE_XML:
        EncodingPrefix = 1266;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONHEADERS_XML:
        EncodingPrefix = 1267;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONLOG_XML:
        EncodingPrefix = 1268;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHAREDSTRINGS_XML:
        EncodingPrefix = 1269;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET: EncodingPrefix =
        1270;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET_MAIN_XML:
        EncodingPrefix = 1271;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEETMETADATA_XML:
        EncodingPrefix = 1272;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_STYLES_XML:
        EncodingPrefix = 1273;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLE_XML:
        EncodingPrefix = 1274;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLESINGLECELLS_XML:
        EncodingPrefix = 1275;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE: EncodingPrefix =
        1276;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE_MAIN_XML:
        EncodingPrefix = 1277;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_USERNAMES_XML:
        EncodingPrefix = 1278;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_VOLATILEDEPENDENCIES_XML: EncodingPrefix = 1279;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_WORKSHEET_XML:
        EncodingPrefix = 1280;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEME_XML: EncodingPrefix = 1281;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEMEOVERRIDE_XML: EncodingPrefix =
        1282;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_VMLDRAWING: EncodingPrefix = 1283;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_COMMENTS_XML:
        EncodingPrefix = 1284;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT:
        EncodingPrefix = 1285;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_GLOSSARY_XML: EncodingPrefix = 1286;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_MAIN_XML:
        EncodingPrefix = 1287;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_ENDNOTES_XML:
        EncodingPrefix = 1288;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FONTTABLE_XML:
        EncodingPrefix = 1289;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTER_XML:
        EncodingPrefix = 1290;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTNOTES_XML:
        EncodingPrefix = 1291;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_NUMBERING_XML:
        EncodingPrefix = 1292;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_SETTINGS_XML:
        EncodingPrefix = 1293;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_STYLES_XML:
        EncodingPrefix = 1294;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE:
        EncodingPrefix = 1295;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE_MAIN_XML:
        EncodingPrefix = 1296;
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_WEBSETTINGS_XML:
        EncodingPrefix = 1297;
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_CORE_PROPERTIES_XML: EncodingPrefix = 1298;
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_DIGITAL_SIGNATURE_XMLSIGNATURE_XML:
        EncodingPrefix = 1299;
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_RELATIONSHIPS_XML: EncodingPrefix = 1300;
    pub const APPLICATION_VND_ORACLE_RESOURCE_JSON: EncodingPrefix = 1301;
    pub const APPLICATION_VND_ORANGE_INDATA: EncodingPrefix = 1302;
    pub const APPLICATION_VND_OSA_NETDEPLOY: EncodingPrefix = 1303;
    pub const APPLICATION_VND_OSGEO_MAPGUIDE_PACKAGE: EncodingPrefix = 1304;
    pub const APPLICATION_VND_OSGI_BUNDLE: EncodingPrefix = 1305;
    pub const APPLICATION_VND_OSGI_DP: EncodingPrefix = 1306;
    pub const APPLICATION_VND_OSGI_SUBSYSTEM: EncodingPrefix = 1307;
    pub const APPLICATION_VND_OTPS_CT_KIP_XML: EncodingPrefix = 1308;
    pub const APPLICATION_VND_OXLI_COUNTGRAPH: EncodingPrefix = 1309;
    pub const APPLICATION_VND_PAGERDUTY_JSON: EncodingPrefix = 1310;
    pub const APPLICATION_VND_PALM: EncodingPrefix = 1311;
    pub const APPLICATION_VND_PANOPLY: EncodingPrefix = 1312;
    pub const APPLICATION_VND_PAOS_XML: EncodingPrefix = 1313;
    pub const APPLICATION_VND_PATENTDIVE: EncodingPrefix = 1314;
    pub const APPLICATION_VND_PATIENTECOMMSDOC: EncodingPrefix = 1315;
    pub const APPLICATION_VND_PAWAAFILE: EncodingPrefix = 1316;
    pub const APPLICATION_VND_PCOS: EncodingPrefix = 1317;
    pub const APPLICATION_VND_PG_FORMAT: EncodingPrefix = 1318;
    pub const APPLICATION_VND_PG_OSASLI: EncodingPrefix = 1319;
    pub const APPLICATION_VND_PIACCESS_APPLICATION_LICENCE: EncodingPrefix = 1320;
    pub const APPLICATION_VND_PICSEL: EncodingPrefix = 1321;
    pub const APPLICATION_VND_PMI_WIDGET: EncodingPrefix = 1322;
    pub const APPLICATION_VND_POC_GROUP_ADVERTISEMENT_XML: EncodingPrefix = 1323;
    pub const APPLICATION_VND_POCKETLEARN: EncodingPrefix = 1324;
    pub const APPLICATION_VND_POWERBUILDER6: EncodingPrefix = 1325;
    pub const APPLICATION_VND_POWERBUILDER6_S: EncodingPrefix = 1326;
    pub const APPLICATION_VND_POWERBUILDER7: EncodingPrefix = 1327;
    pub const APPLICATION_VND_POWERBUILDER7_S: EncodingPrefix = 1328;
    pub const APPLICATION_VND_POWERBUILDER75: EncodingPrefix = 1329;
    pub const APPLICATION_VND_POWERBUILDER75_S: EncodingPrefix = 1330;
    pub const APPLICATION_VND_PREMINET: EncodingPrefix = 1331;
    pub const APPLICATION_VND_PREVIEWSYSTEMS_BOX: EncodingPrefix = 1332;
    pub const APPLICATION_VND_PROTEUS_MAGAZINE: EncodingPrefix = 1333;
    pub const APPLICATION_VND_PSFS: EncodingPrefix = 1334;
    pub const APPLICATION_VND_PT_MUNDUSMUNDI: EncodingPrefix = 1335;
    pub const APPLICATION_VND_PUBLISHARE_DELTA_TREE: EncodingPrefix = 1336;
    pub const APPLICATION_VND_PVI_PTID1: EncodingPrefix = 1337;
    pub const APPLICATION_VND_PWG_MULTIPLEXED: EncodingPrefix = 1338;
    pub const APPLICATION_VND_PWG_XHTML_PRINT_XML: EncodingPrefix = 1339;
    pub const APPLICATION_VND_QUALCOMM_BREW_APP_RES: EncodingPrefix = 1340;
    pub const APPLICATION_VND_QUARANTAINENET: EncodingPrefix = 1341;
    pub const APPLICATION_VND_QUOBJECT_QUOXDOCUMENT: EncodingPrefix = 1342;
    pub const APPLICATION_VND_RADISYS_MOML_XML: EncodingPrefix = 1343;
    pub const APPLICATION_VND_RADISYS_MSML_XML: EncodingPrefix = 1344;
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_XML: EncodingPrefix = 1345;
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_CONF_XML: EncodingPrefix = 1346;
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_CONN_XML: EncodingPrefix = 1347;
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_DIALOG_XML: EncodingPrefix = 1348;
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_STREAM_XML: EncodingPrefix = 1349;
    pub const APPLICATION_VND_RADISYS_MSML_CONF_XML: EncodingPrefix = 1350;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_XML: EncodingPrefix = 1351;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_BASE_XML: EncodingPrefix = 1352;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_DETECT_XML: EncodingPrefix = 1353;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_SENDRECV_XML: EncodingPrefix = 1354;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_GROUP_XML: EncodingPrefix = 1355;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_SPEECH_XML: EncodingPrefix = 1356;
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_TRANSFORM_XML: EncodingPrefix = 1357;
    pub const APPLICATION_VND_RAINSTOR_DATA: EncodingPrefix = 1358;
    pub const APPLICATION_VND_RAPID: EncodingPrefix = 1359;
    pub const APPLICATION_VND_RAR: EncodingPrefix = 1360;
    pub const APPLICATION_VND_REALVNC_BED: EncodingPrefix = 1361;
    pub const APPLICATION_VND_RECORDARE_MUSICXML: EncodingPrefix = 1362;
    pub const APPLICATION_VND_RECORDARE_MUSICXML_XML: EncodingPrefix = 1363;
    pub const APPLICATION_VND_RELPIPE: EncodingPrefix = 1364;
    pub const APPLICATION_VND_RESILIENT_LOGIC: EncodingPrefix = 1365;
    pub const APPLICATION_VND_RESTFUL_JSON: EncodingPrefix = 1366;
    pub const APPLICATION_VND_RIG_CRYPTONOTE: EncodingPrefix = 1367;
    pub const APPLICATION_VND_ROUTE66_LINK66_XML: EncodingPrefix = 1368;
    pub const APPLICATION_VND_RS_274X: EncodingPrefix = 1369;
    pub const APPLICATION_VND_RUCKUS_DOWNLOAD: EncodingPrefix = 1370;
    pub const APPLICATION_VND_S3SMS: EncodingPrefix = 1371;
    pub const APPLICATION_VND_SAILINGTRACKER_TRACK: EncodingPrefix = 1372;
    pub const APPLICATION_VND_SAR: EncodingPrefix = 1373;
    pub const APPLICATION_VND_SBM_CID: EncodingPrefix = 1374;
    pub const APPLICATION_VND_SBM_MID2: EncodingPrefix = 1375;
    pub const APPLICATION_VND_SCRIBUS: EncodingPrefix = 1376;
    pub const APPLICATION_VND_SEALED_3DF: EncodingPrefix = 1377;
    pub const APPLICATION_VND_SEALED_CSF: EncodingPrefix = 1378;
    pub const APPLICATION_VND_SEALED_DOC: EncodingPrefix = 1379;
    pub const APPLICATION_VND_SEALED_EML: EncodingPrefix = 1380;
    pub const APPLICATION_VND_SEALED_MHT: EncodingPrefix = 1381;
    pub const APPLICATION_VND_SEALED_NET: EncodingPrefix = 1382;
    pub const APPLICATION_VND_SEALED_PPT: EncodingPrefix = 1383;
    pub const APPLICATION_VND_SEALED_TIFF: EncodingPrefix = 1384;
    pub const APPLICATION_VND_SEALED_XLS: EncodingPrefix = 1385;
    pub const APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_HTML: EncodingPrefix = 1386;
    pub const APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_PDF: EncodingPrefix = 1387;
    pub const APPLICATION_VND_SEEMAIL: EncodingPrefix = 1388;
    pub const APPLICATION_VND_SEIS_JSON: EncodingPrefix = 1389;
    pub const APPLICATION_VND_SEMA: EncodingPrefix = 1390;
    pub const APPLICATION_VND_SEMD: EncodingPrefix = 1391;
    pub const APPLICATION_VND_SEMF: EncodingPrefix = 1392;
    pub const APPLICATION_VND_SHADE_SAVE_FILE: EncodingPrefix = 1393;
    pub const APPLICATION_VND_SHANA_INFORMED_FORMDATA: EncodingPrefix = 1394;
    pub const APPLICATION_VND_SHANA_INFORMED_FORMTEMPLATE: EncodingPrefix = 1395;
    pub const APPLICATION_VND_SHANA_INFORMED_INTERCHANGE: EncodingPrefix = 1396;
    pub const APPLICATION_VND_SHANA_INFORMED_PACKAGE: EncodingPrefix = 1397;
    pub const APPLICATION_VND_SHOOTPROOF_JSON: EncodingPrefix = 1398;
    pub const APPLICATION_VND_SHOPKICK_JSON: EncodingPrefix = 1399;
    pub const APPLICATION_VND_SHP: EncodingPrefix = 1400;
    pub const APPLICATION_VND_SHX: EncodingPrefix = 1401;
    pub const APPLICATION_VND_SIGROK_SESSION: EncodingPrefix = 1402;
    pub const APPLICATION_VND_SIREN_JSON: EncodingPrefix = 1403;
    pub const APPLICATION_VND_SMAF: EncodingPrefix = 1404;
    pub const APPLICATION_VND_SMART_NOTEBOOK: EncodingPrefix = 1405;
    pub const APPLICATION_VND_SMART_TEACHER: EncodingPrefix = 1406;
    pub const APPLICATION_VND_SMINTIO_PORTALS_ARCHIVE: EncodingPrefix = 1407;
    pub const APPLICATION_VND_SNESDEV_PAGE_TABLE: EncodingPrefix = 1408;
    pub const APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML: EncodingPrefix = 1409;
    pub const APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML_ZIP: EncodingPrefix = 1410;
    pub const APPLICATION_VND_SOLENT_SDKM_XML: EncodingPrefix = 1411;
    pub const APPLICATION_VND_SPOTFIRE_DXP: EncodingPrefix = 1412;
    pub const APPLICATION_VND_SPOTFIRE_SFS: EncodingPrefix = 1413;
    pub const APPLICATION_VND_SQLITE3: EncodingPrefix = 1414;
    pub const APPLICATION_VND_SSS_COD: EncodingPrefix = 1415;
    pub const APPLICATION_VND_SSS_DTF: EncodingPrefix = 1416;
    pub const APPLICATION_VND_SSS_NTF: EncodingPrefix = 1417;
    pub const APPLICATION_VND_STEPMANIA_PACKAGE: EncodingPrefix = 1418;
    pub const APPLICATION_VND_STEPMANIA_STEPCHART: EncodingPrefix = 1419;
    pub const APPLICATION_VND_STREET_STREAM: EncodingPrefix = 1420;
    pub const APPLICATION_VND_SUN_WADL_XML: EncodingPrefix = 1421;
    pub const APPLICATION_VND_SUS_CALENDAR: EncodingPrefix = 1422;
    pub const APPLICATION_VND_SVD: EncodingPrefix = 1423;
    pub const APPLICATION_VND_SWIFTVIEW_ICS: EncodingPrefix = 1424;
    pub const APPLICATION_VND_SYBYL_MOL2: EncodingPrefix = 1425;
    pub const APPLICATION_VND_SYCLE_XML: EncodingPrefix = 1426;
    pub const APPLICATION_VND_SYFT_JSON: EncodingPrefix = 1427;
    pub const APPLICATION_VND_SYNCML_XML: EncodingPrefix = 1428;
    pub const APPLICATION_VND_SYNCML_DM_WBXML: EncodingPrefix = 1429;
    pub const APPLICATION_VND_SYNCML_DM_XML: EncodingPrefix = 1430;
    pub const APPLICATION_VND_SYNCML_DM_NOTIFICATION: EncodingPrefix = 1431;
    pub const APPLICATION_VND_SYNCML_DMDDF_WBXML: EncodingPrefix = 1432;
    pub const APPLICATION_VND_SYNCML_DMDDF_XML: EncodingPrefix = 1433;
    pub const APPLICATION_VND_SYNCML_DMTNDS_WBXML: EncodingPrefix = 1434;
    pub const APPLICATION_VND_SYNCML_DMTNDS_XML: EncodingPrefix = 1435;
    pub const APPLICATION_VND_SYNCML_DS_NOTIFICATION: EncodingPrefix = 1436;
    pub const APPLICATION_VND_TABLESCHEMA_JSON: EncodingPrefix = 1437;
    pub const APPLICATION_VND_TAO_INTENT_MODULE_ARCHIVE: EncodingPrefix = 1438;
    pub const APPLICATION_VND_TCPDUMP_PCAP: EncodingPrefix = 1439;
    pub const APPLICATION_VND_THINK_CELL_PPTTC_JSON: EncodingPrefix = 1440;
    pub const APPLICATION_VND_TMD_MEDIAFLEX_API_XML: EncodingPrefix = 1441;
    pub const APPLICATION_VND_TML: EncodingPrefix = 1442;
    pub const APPLICATION_VND_TMOBILE_LIVETV: EncodingPrefix = 1443;
    pub const APPLICATION_VND_TRI_ONESOURCE: EncodingPrefix = 1444;
    pub const APPLICATION_VND_TRID_TPT: EncodingPrefix = 1445;
    pub const APPLICATION_VND_TRISCAPE_MXS: EncodingPrefix = 1446;
    pub const APPLICATION_VND_TRUEAPP: EncodingPrefix = 1447;
    pub const APPLICATION_VND_TRUEDOC: EncodingPrefix = 1448;
    pub const APPLICATION_VND_UBISOFT_WEBPLAYER: EncodingPrefix = 1449;
    pub const APPLICATION_VND_UFDL: EncodingPrefix = 1450;
    pub const APPLICATION_VND_UIQ_THEME: EncodingPrefix = 1451;
    pub const APPLICATION_VND_UMAJIN: EncodingPrefix = 1452;
    pub const APPLICATION_VND_UNITY: EncodingPrefix = 1453;
    pub const APPLICATION_VND_UOML_XML: EncodingPrefix = 1454;
    pub const APPLICATION_VND_UPLANET_ALERT: EncodingPrefix = 1455;
    pub const APPLICATION_VND_UPLANET_ALERT_WBXML: EncodingPrefix = 1456;
    pub const APPLICATION_VND_UPLANET_BEARER_CHOICE: EncodingPrefix = 1457;
    pub const APPLICATION_VND_UPLANET_BEARER_CHOICE_WBXML: EncodingPrefix = 1458;
    pub const APPLICATION_VND_UPLANET_CACHEOP: EncodingPrefix = 1459;
    pub const APPLICATION_VND_UPLANET_CACHEOP_WBXML: EncodingPrefix = 1460;
    pub const APPLICATION_VND_UPLANET_CHANNEL: EncodingPrefix = 1461;
    pub const APPLICATION_VND_UPLANET_CHANNEL_WBXML: EncodingPrefix = 1462;
    pub const APPLICATION_VND_UPLANET_LIST: EncodingPrefix = 1463;
    pub const APPLICATION_VND_UPLANET_LIST_WBXML: EncodingPrefix = 1464;
    pub const APPLICATION_VND_UPLANET_LISTCMD: EncodingPrefix = 1465;
    pub const APPLICATION_VND_UPLANET_LISTCMD_WBXML: EncodingPrefix = 1466;
    pub const APPLICATION_VND_UPLANET_SIGNAL: EncodingPrefix = 1467;
    pub const APPLICATION_VND_URI_MAP: EncodingPrefix = 1468;
    pub const APPLICATION_VND_VALVE_SOURCE_MATERIAL: EncodingPrefix = 1469;
    pub const APPLICATION_VND_VCX: EncodingPrefix = 1470;
    pub const APPLICATION_VND_VD_STUDY: EncodingPrefix = 1471;
    pub const APPLICATION_VND_VECTORWORKS: EncodingPrefix = 1472;
    pub const APPLICATION_VND_VEL_JSON: EncodingPrefix = 1473;
    pub const APPLICATION_VND_VERIMATRIX_VCAS: EncodingPrefix = 1474;
    pub const APPLICATION_VND_VERITONE_AION_JSON: EncodingPrefix = 1475;
    pub const APPLICATION_VND_VERYANT_THIN: EncodingPrefix = 1476;
    pub const APPLICATION_VND_VES_ENCRYPTED: EncodingPrefix = 1477;
    pub const APPLICATION_VND_VIDSOFT_VIDCONFERENCE: EncodingPrefix = 1478;
    pub const APPLICATION_VND_VISIO: EncodingPrefix = 1479;
    pub const APPLICATION_VND_VISIONARY: EncodingPrefix = 1480;
    pub const APPLICATION_VND_VIVIDENCE_SCRIPTFILE: EncodingPrefix = 1481;
    pub const APPLICATION_VND_VSF: EncodingPrefix = 1482;
    pub const APPLICATION_VND_WAP_SIC: EncodingPrefix = 1483;
    pub const APPLICATION_VND_WAP_SLC: EncodingPrefix = 1484;
    pub const APPLICATION_VND_WAP_WBXML: EncodingPrefix = 1485;
    pub const APPLICATION_VND_WAP_WMLC: EncodingPrefix = 1486;
    pub const APPLICATION_VND_WAP_WMLSCRIPTC: EncodingPrefix = 1487;
    pub const APPLICATION_VND_WASMFLOW_WAFL: EncodingPrefix = 1488;
    pub const APPLICATION_VND_WEBTURBO: EncodingPrefix = 1489;
    pub const APPLICATION_VND_WFA_DPP: EncodingPrefix = 1490;
    pub const APPLICATION_VND_WFA_P2P: EncodingPrefix = 1491;
    pub const APPLICATION_VND_WFA_WSC: EncodingPrefix = 1492;
    pub const APPLICATION_VND_WINDOWS_DEVICEPAIRING: EncodingPrefix = 1493;
    pub const APPLICATION_VND_WMC: EncodingPrefix = 1494;
    pub const APPLICATION_VND_WMF_BOOTSTRAP: EncodingPrefix = 1495;
    pub const APPLICATION_VND_WOLFRAM_MATHEMATICA: EncodingPrefix = 1496;
    pub const APPLICATION_VND_WOLFRAM_MATHEMATICA_PACKAGE: EncodingPrefix = 1497;
    pub const APPLICATION_VND_WOLFRAM_PLAYER: EncodingPrefix = 1498;
    pub const APPLICATION_VND_WORDLIFT: EncodingPrefix = 1499;
    pub const APPLICATION_VND_WORDPERFECT: EncodingPrefix = 1500;
    pub const APPLICATION_VND_WQD: EncodingPrefix = 1501;
    pub const APPLICATION_VND_WRQ_HP3000_LABELLED: EncodingPrefix = 1502;
    pub const APPLICATION_VND_WT_STF: EncodingPrefix = 1503;
    pub const APPLICATION_VND_WV_CSP_WBXML: EncodingPrefix = 1504;
    pub const APPLICATION_VND_WV_CSP_XML: EncodingPrefix = 1505;
    pub const APPLICATION_VND_WV_SSP_XML: EncodingPrefix = 1506;
    pub const APPLICATION_VND_XACML_JSON: EncodingPrefix = 1507;
    pub const APPLICATION_VND_XARA: EncodingPrefix = 1508;
    pub const APPLICATION_VND_XECRETS_ENCRYPTED: EncodingPrefix = 1509;
    pub const APPLICATION_VND_XFDL: EncodingPrefix = 1510;
    pub const APPLICATION_VND_XFDL_WEBFORM: EncodingPrefix = 1511;
    pub const APPLICATION_VND_XMI_XML: EncodingPrefix = 1512;
    pub const APPLICATION_VND_XMPIE_CPKG: EncodingPrefix = 1513;
    pub const APPLICATION_VND_XMPIE_DPKG: EncodingPrefix = 1514;
    pub const APPLICATION_VND_XMPIE_PLAN: EncodingPrefix = 1515;
    pub const APPLICATION_VND_XMPIE_PPKG: EncodingPrefix = 1516;
    pub const APPLICATION_VND_XMPIE_XLIM: EncodingPrefix = 1517;
    pub const APPLICATION_VND_YAMAHA_HV_DIC: EncodingPrefix = 1518;
    pub const APPLICATION_VND_YAMAHA_HV_SCRIPT: EncodingPrefix = 1519;
    pub const APPLICATION_VND_YAMAHA_HV_VOICE: EncodingPrefix = 1520;
    pub const APPLICATION_VND_YAMAHA_OPENSCOREFORMAT: EncodingPrefix = 1521;
    pub const APPLICATION_VND_YAMAHA_OPENSCOREFORMAT_OSFPVG_XML: EncodingPrefix = 1522;
    pub const APPLICATION_VND_YAMAHA_REMOTE_SETUP: EncodingPrefix = 1523;
    pub const APPLICATION_VND_YAMAHA_SMAF_AUDIO: EncodingPrefix = 1524;
    pub const APPLICATION_VND_YAMAHA_SMAF_PHRASE: EncodingPrefix = 1525;
    pub const APPLICATION_VND_YAMAHA_THROUGH_NGN: EncodingPrefix = 1526;
    pub const APPLICATION_VND_YAMAHA_TUNNEL_UDPENCAP: EncodingPrefix = 1527;
    pub const APPLICATION_VND_YAOWEME: EncodingPrefix = 1528;
    pub const APPLICATION_VND_YELLOWRIVER_CUSTOM_MENU: EncodingPrefix = 1529;
    pub const APPLICATION_VND_YOUTUBE_YT: EncodingPrefix = 1530;
    pub const APPLICATION_VND_ZUL: EncodingPrefix = 1531;
    pub const APPLICATION_VND_ZZAZZ_DECK_XML: EncodingPrefix = 1532;
    pub const APPLICATION_VOICEXML_XML: EncodingPrefix = 1533;
    pub const APPLICATION_VOUCHER_CMS_JSON: EncodingPrefix = 1534;
    pub const APPLICATION_VQ_RTCPXR: EncodingPrefix = 1535;
    pub const APPLICATION_WASM: EncodingPrefix = 1536;
    pub const APPLICATION_WATCHERINFO_XML: EncodingPrefix = 1537;
    pub const APPLICATION_WEBPUSH_OPTIONS_JSON: EncodingPrefix = 1538;
    pub const APPLICATION_WHOISPP_QUERY: EncodingPrefix = 1539;
    pub const APPLICATION_WHOISPP_RESPONSE: EncodingPrefix = 1540;
    pub const APPLICATION_WIDGET: EncodingPrefix = 1541;
    pub const APPLICATION_WITA: EncodingPrefix = 1542;
    pub const APPLICATION_WORDPERFECT5_1: EncodingPrefix = 1543;
    pub const APPLICATION_WSDL_XML: EncodingPrefix = 1544;
    pub const APPLICATION_WSPOLICY_XML: EncodingPrefix = 1545;
    pub const APPLICATION_X_PKI_MESSAGE: EncodingPrefix = 1546;
    pub const APPLICATION_X_WWW_FORM_URLENCODED: EncodingPrefix = 1547;
    pub const APPLICATION_X_X509_CA_CERT: EncodingPrefix = 1548;
    pub const APPLICATION_X_X509_CA_RA_CERT: EncodingPrefix = 1549;
    pub const APPLICATION_X_X509_NEXT_CA_CERT: EncodingPrefix = 1550;
    pub const APPLICATION_X400_BP: EncodingPrefix = 1551;
    pub const APPLICATION_XACML_XML: EncodingPrefix = 1552;
    pub const APPLICATION_XCAP_ATT_XML: EncodingPrefix = 1553;
    pub const APPLICATION_XCAP_CAPS_XML: EncodingPrefix = 1554;
    pub const APPLICATION_XCAP_DIFF_XML: EncodingPrefix = 1555;
    pub const APPLICATION_XCAP_EL_XML: EncodingPrefix = 1556;
    pub const APPLICATION_XCAP_ERROR_XML: EncodingPrefix = 1557;
    pub const APPLICATION_XCAP_NS_XML: EncodingPrefix = 1558;
    pub const APPLICATION_XCON_CONFERENCE_INFO_XML: EncodingPrefix = 1559;
    pub const APPLICATION_XCON_CONFERENCE_INFO_DIFF_XML: EncodingPrefix = 1560;
    pub const APPLICATION_XENC_XML: EncodingPrefix = 1561;
    pub const APPLICATION_XFDF: EncodingPrefix = 1562;
    pub const APPLICATION_XHTML_XML: EncodingPrefix = 1563;
    pub const APPLICATION_XLIFF_XML: EncodingPrefix = 1564;
    pub const APPLICATION_XML: EncodingPrefix = 1565;
    pub const APPLICATION_XML_DTD: EncodingPrefix = 1566;
    pub const APPLICATION_XML_EXTERNAL_PARSED_ENTITY: EncodingPrefix = 1567;
    pub const APPLICATION_XML_PATCH_XML: EncodingPrefix = 1568;
    pub const APPLICATION_XMPP_XML: EncodingPrefix = 1569;
    pub const APPLICATION_XOP_XML: EncodingPrefix = 1570;
    pub const APPLICATION_XSLT_XML: EncodingPrefix = 1571;
    pub const APPLICATION_XV_XML: EncodingPrefix = 1572;
    pub const APPLICATION_YAML: EncodingPrefix = 1573;
    pub const APPLICATION_YANG: EncodingPrefix = 1574;
    pub const APPLICATION_YANG_DATA_CBOR: EncodingPrefix = 1575;
    pub const APPLICATION_YANG_DATA_JSON: EncodingPrefix = 1576;
    pub const APPLICATION_YANG_DATA_XML: EncodingPrefix = 1577;
    pub const APPLICATION_YANG_PATCH_JSON: EncodingPrefix = 1578;
    pub const APPLICATION_YANG_PATCH_XML: EncodingPrefix = 1579;
    pub const APPLICATION_YANG_SID_JSON: EncodingPrefix = 1580;
    pub const APPLICATION_YIN_XML: EncodingPrefix = 1581;
    pub const APPLICATION_ZIP: EncodingPrefix = 1582;
    pub const APPLICATION_ZLIB: EncodingPrefix = 1583;
    pub const APPLICATION_ZSTD: EncodingPrefix = 1584;
    pub const AUDIO_1D_INTERLEAVED_PARITYFEC: EncodingPrefix = 1585;
    pub const AUDIO_32KADPCM: EncodingPrefix = 1586;
    pub const AUDIO_3GPP: EncodingPrefix = 1587;
    pub const AUDIO_3GPP2: EncodingPrefix = 1588;
    pub const AUDIO_AMR: EncodingPrefix = 1589;
    pub const AUDIO_AMR_WB: EncodingPrefix = 1590;
    pub const AUDIO_ATRAC_ADVANCED_LOSSLESS: EncodingPrefix = 1591;
    pub const AUDIO_ATRAC_X: EncodingPrefix = 1592;
    pub const AUDIO_ATRAC3: EncodingPrefix = 1593;
    pub const AUDIO_BV16: EncodingPrefix = 1594;
    pub const AUDIO_BV32: EncodingPrefix = 1595;
    pub const AUDIO_CN: EncodingPrefix = 1596;
    pub const AUDIO_DAT12: EncodingPrefix = 1597;
    pub const AUDIO_DV: EncodingPrefix = 1598;
    pub const AUDIO_DVI4: EncodingPrefix = 1599;
    pub const AUDIO_EVRC: EncodingPrefix = 1600;
    pub const AUDIO_EVRC_QCP: EncodingPrefix = 1601;
    pub const AUDIO_EVRC0: EncodingPrefix = 1602;
    pub const AUDIO_EVRC1: EncodingPrefix = 1603;
    pub const AUDIO_EVRCB: EncodingPrefix = 1604;
    pub const AUDIO_EVRCB0: EncodingPrefix = 1605;
    pub const AUDIO_EVRCB1: EncodingPrefix = 1606;
    pub const AUDIO_EVRCNW: EncodingPrefix = 1607;
    pub const AUDIO_EVRCNW0: EncodingPrefix = 1608;
    pub const AUDIO_EVRCNW1: EncodingPrefix = 1609;
    pub const AUDIO_EVRCWB: EncodingPrefix = 1610;
    pub const AUDIO_EVRCWB0: EncodingPrefix = 1611;
    pub const AUDIO_EVRCWB1: EncodingPrefix = 1612;
    pub const AUDIO_EVS: EncodingPrefix = 1613;
    pub const AUDIO_G711_0: EncodingPrefix = 1614;
    pub const AUDIO_G719: EncodingPrefix = 1615;
    pub const AUDIO_G722: EncodingPrefix = 1616;
    pub const AUDIO_G7221: EncodingPrefix = 1617;
    pub const AUDIO_G723: EncodingPrefix = 1618;
    pub const AUDIO_G726_16: EncodingPrefix = 1619;
    pub const AUDIO_G726_24: EncodingPrefix = 1620;
    pub const AUDIO_G726_32: EncodingPrefix = 1621;
    pub const AUDIO_G726_40: EncodingPrefix = 1622;
    pub const AUDIO_G728: EncodingPrefix = 1623;
    pub const AUDIO_G729: EncodingPrefix = 1624;
    pub const AUDIO_G7291: EncodingPrefix = 1625;
    pub const AUDIO_G729D: EncodingPrefix = 1626;
    pub const AUDIO_G729E: EncodingPrefix = 1627;
    pub const AUDIO_GSM: EncodingPrefix = 1628;
    pub const AUDIO_GSM_EFR: EncodingPrefix = 1629;
    pub const AUDIO_GSM_HR_08: EncodingPrefix = 1630;
    pub const AUDIO_L16: EncodingPrefix = 1631;
    pub const AUDIO_L20: EncodingPrefix = 1632;
    pub const AUDIO_L24: EncodingPrefix = 1633;
    pub const AUDIO_L8: EncodingPrefix = 1634;
    pub const AUDIO_LPC: EncodingPrefix = 1635;
    pub const AUDIO_MELP: EncodingPrefix = 1636;
    pub const AUDIO_MELP1200: EncodingPrefix = 1637;
    pub const AUDIO_MELP2400: EncodingPrefix = 1638;
    pub const AUDIO_MELP600: EncodingPrefix = 1639;
    pub const AUDIO_MP4A_LATM: EncodingPrefix = 1640;
    pub const AUDIO_MPA: EncodingPrefix = 1641;
    pub const AUDIO_PCMA: EncodingPrefix = 1642;
    pub const AUDIO_PCMA_WB: EncodingPrefix = 1643;
    pub const AUDIO_PCMU: EncodingPrefix = 1644;
    pub const AUDIO_PCMU_WB: EncodingPrefix = 1645;
    pub const AUDIO_QCELP: EncodingPrefix = 1646;
    pub const AUDIO_RED: EncodingPrefix = 1647;
    pub const AUDIO_SMV: EncodingPrefix = 1648;
    pub const AUDIO_SMV_QCP: EncodingPrefix = 1649;
    pub const AUDIO_SMV0: EncodingPrefix = 1650;
    pub const AUDIO_TETRA_ACELP: EncodingPrefix = 1651;
    pub const AUDIO_TETRA_ACELP_BB: EncodingPrefix = 1652;
    pub const AUDIO_TSVCIS: EncodingPrefix = 1653;
    pub const AUDIO_UEMCLIP: EncodingPrefix = 1654;
    pub const AUDIO_VDVI: EncodingPrefix = 1655;
    pub const AUDIO_VMR_WB: EncodingPrefix = 1656;
    pub const AUDIO_AAC: EncodingPrefix = 1657;
    pub const AUDIO_AC3: EncodingPrefix = 1658;
    pub const AUDIO_AMR_WB_P: EncodingPrefix = 1659;
    pub const AUDIO_APTX: EncodingPrefix = 1660;
    pub const AUDIO_ASC: EncodingPrefix = 1661;
    pub const AUDIO_BASIC: EncodingPrefix = 1662;
    pub const AUDIO_CLEARMODE: EncodingPrefix = 1663;
    pub const AUDIO_DLS: EncodingPrefix = 1664;
    pub const AUDIO_DSR_ES201108: EncodingPrefix = 1665;
    pub const AUDIO_DSR_ES202050: EncodingPrefix = 1666;
    pub const AUDIO_DSR_ES202211: EncodingPrefix = 1667;
    pub const AUDIO_DSR_ES202212: EncodingPrefix = 1668;
    pub const AUDIO_EAC3: EncodingPrefix = 1669;
    pub const AUDIO_ENCAPRTP: EncodingPrefix = 1670;
    pub const AUDIO_EXAMPLE: EncodingPrefix = 1671;
    pub const AUDIO_FLEXFEC: EncodingPrefix = 1672;
    pub const AUDIO_FWDRED: EncodingPrefix = 1673;
    pub const AUDIO_ILBC: EncodingPrefix = 1674;
    pub const AUDIO_IP_MR_V2_5: EncodingPrefix = 1675;
    pub const AUDIO_MATROSKA: EncodingPrefix = 1676;
    pub const AUDIO_MHAS: EncodingPrefix = 1677;
    pub const AUDIO_MOBILE_XMF: EncodingPrefix = 1678;
    pub const AUDIO_MP4: EncodingPrefix = 1679;
    pub const AUDIO_MPA_ROBUST: EncodingPrefix = 1680;
    pub const AUDIO_MPEG: EncodingPrefix = 1681;
    pub const AUDIO_MPEG4_GENERIC: EncodingPrefix = 1682;
    pub const AUDIO_OGG: EncodingPrefix = 1683;
    pub const AUDIO_OPUS: EncodingPrefix = 1684;
    pub const AUDIO_PARITYFEC: EncodingPrefix = 1685;
    pub const AUDIO_PRS_SID: EncodingPrefix = 1686;
    pub const AUDIO_RAPTORFEC: EncodingPrefix = 1687;
    pub const AUDIO_RTP_ENC_AESCM128: EncodingPrefix = 1688;
    pub const AUDIO_RTP_MIDI: EncodingPrefix = 1689;
    pub const AUDIO_RTPLOOPBACK: EncodingPrefix = 1690;
    pub const AUDIO_RTX: EncodingPrefix = 1691;
    pub const AUDIO_SCIP: EncodingPrefix = 1692;
    pub const AUDIO_SOFA: EncodingPrefix = 1693;
    pub const AUDIO_SP_MIDI: EncodingPrefix = 1694;
    pub const AUDIO_SPEEX: EncodingPrefix = 1695;
    pub const AUDIO_T140C: EncodingPrefix = 1696;
    pub const AUDIO_T38: EncodingPrefix = 1697;
    pub const AUDIO_TELEPHONE_EVENT: EncodingPrefix = 1698;
    pub const AUDIO_TONE: EncodingPrefix = 1699;
    pub const AUDIO_ULPFEC: EncodingPrefix = 1700;
    pub const AUDIO_USAC: EncodingPrefix = 1701;
    pub const AUDIO_VND_3GPP_IUFP: EncodingPrefix = 1702;
    pub const AUDIO_VND_4SB: EncodingPrefix = 1703;
    pub const AUDIO_VND_CELP: EncodingPrefix = 1704;
    pub const AUDIO_VND_AUDIOKOZ: EncodingPrefix = 1705;
    pub const AUDIO_VND_CISCO_NSE: EncodingPrefix = 1706;
    pub const AUDIO_VND_CMLES_RADIO_EVENTS: EncodingPrefix = 1707;
    pub const AUDIO_VND_CNS_ANP1: EncodingPrefix = 1708;
    pub const AUDIO_VND_CNS_INF1: EncodingPrefix = 1709;
    pub const AUDIO_VND_DECE_AUDIO: EncodingPrefix = 1710;
    pub const AUDIO_VND_DIGITAL_WINDS: EncodingPrefix = 1711;
    pub const AUDIO_VND_DLNA_ADTS: EncodingPrefix = 1712;
    pub const AUDIO_VND_DOLBY_HEAAC_1: EncodingPrefix = 1713;
    pub const AUDIO_VND_DOLBY_HEAAC_2: EncodingPrefix = 1714;
    pub const AUDIO_VND_DOLBY_MLP: EncodingPrefix = 1715;
    pub const AUDIO_VND_DOLBY_MPS: EncodingPrefix = 1716;
    pub const AUDIO_VND_DOLBY_PL2: EncodingPrefix = 1717;
    pub const AUDIO_VND_DOLBY_PL2X: EncodingPrefix = 1718;
    pub const AUDIO_VND_DOLBY_PL2Z: EncodingPrefix = 1719;
    pub const AUDIO_VND_DOLBY_PULSE_1: EncodingPrefix = 1720;
    pub const AUDIO_VND_DRA: EncodingPrefix = 1721;
    pub const AUDIO_VND_DTS: EncodingPrefix = 1722;
    pub const AUDIO_VND_DTS_HD: EncodingPrefix = 1723;
    pub const AUDIO_VND_DTS_UHD: EncodingPrefix = 1724;
    pub const AUDIO_VND_DVB_FILE: EncodingPrefix = 1725;
    pub const AUDIO_VND_EVERAD_PLJ: EncodingPrefix = 1726;
    pub const AUDIO_VND_HNS_AUDIO: EncodingPrefix = 1727;
    pub const AUDIO_VND_LUCENT_VOICE: EncodingPrefix = 1728;
    pub const AUDIO_VND_MS_PLAYREADY_MEDIA_PYA: EncodingPrefix = 1729;
    pub const AUDIO_VND_NOKIA_MOBILE_XMF: EncodingPrefix = 1730;
    pub const AUDIO_VND_NORTEL_VBK: EncodingPrefix = 1731;
    pub const AUDIO_VND_NUERA_ECELP4800: EncodingPrefix = 1732;
    pub const AUDIO_VND_NUERA_ECELP7470: EncodingPrefix = 1733;
    pub const AUDIO_VND_NUERA_ECELP9600: EncodingPrefix = 1734;
    pub const AUDIO_VND_OCTEL_SBC: EncodingPrefix = 1735;
    pub const AUDIO_VND_PRESONUS_MULTITRACK: EncodingPrefix = 1736;
    pub const AUDIO_VND_QCELP: EncodingPrefix = 1737;
    pub const AUDIO_VND_RHETOREX_32KADPCM: EncodingPrefix = 1738;
    pub const AUDIO_VND_RIP: EncodingPrefix = 1739;
    pub const AUDIO_VND_SEALEDMEDIA_SOFTSEAL_MPEG: EncodingPrefix = 1740;
    pub const AUDIO_VND_VMX_CVSD: EncodingPrefix = 1741;
    pub const AUDIO_VORBIS: EncodingPrefix = 1742;
    pub const AUDIO_VORBIS_CONFIG: EncodingPrefix = 1743;
    pub const FONT_COLLECTION: EncodingPrefix = 1744;
    pub const FONT_OTF: EncodingPrefix = 1745;
    pub const FONT_SFNT: EncodingPrefix = 1746;
    pub const FONT_TTF: EncodingPrefix = 1747;
    pub const FONT_WOFF: EncodingPrefix = 1748;
    pub const FONT_WOFF2: EncodingPrefix = 1749;
    pub const IMAGE_ACES: EncodingPrefix = 1750;
    pub const IMAGE_APNG: EncodingPrefix = 1751;
    pub const IMAGE_AVCI: EncodingPrefix = 1752;
    pub const IMAGE_AVCS: EncodingPrefix = 1753;
    pub const IMAGE_AVIF: EncodingPrefix = 1754;
    pub const IMAGE_BMP: EncodingPrefix = 1755;
    pub const IMAGE_CGM: EncodingPrefix = 1756;
    pub const IMAGE_DICOM_RLE: EncodingPrefix = 1757;
    pub const IMAGE_DPX: EncodingPrefix = 1758;
    pub const IMAGE_EMF: EncodingPrefix = 1759;
    pub const IMAGE_EXAMPLE: EncodingPrefix = 1760;
    pub const IMAGE_FITS: EncodingPrefix = 1761;
    pub const IMAGE_G3FAX: EncodingPrefix = 1762;
    pub const IMAGE_GIF: EncodingPrefix = 1763;
    pub const IMAGE_HEIC: EncodingPrefix = 1764;
    pub const IMAGE_HEIC_SEQUENCE: EncodingPrefix = 1765;
    pub const IMAGE_HEIF: EncodingPrefix = 1766;
    pub const IMAGE_HEIF_SEQUENCE: EncodingPrefix = 1767;
    pub const IMAGE_HEJ2K: EncodingPrefix = 1768;
    pub const IMAGE_HSJ2: EncodingPrefix = 1769;
    pub const IMAGE_IEF: EncodingPrefix = 1770;
    pub const IMAGE_J2C: EncodingPrefix = 1771;
    pub const IMAGE_JLS: EncodingPrefix = 1772;
    pub const IMAGE_JP2: EncodingPrefix = 1773;
    pub const IMAGE_JPEG: EncodingPrefix = 1774;
    pub const IMAGE_JPH: EncodingPrefix = 1775;
    pub const IMAGE_JPHC: EncodingPrefix = 1776;
    pub const IMAGE_JPM: EncodingPrefix = 1777;
    pub const IMAGE_JPX: EncodingPrefix = 1778;
    pub const IMAGE_JXR: EncodingPrefix = 1779;
    pub const IMAGE_JXRA: EncodingPrefix = 1780;
    pub const IMAGE_JXRS: EncodingPrefix = 1781;
    pub const IMAGE_JXS: EncodingPrefix = 1782;
    pub const IMAGE_JXSC: EncodingPrefix = 1783;
    pub const IMAGE_JXSI: EncodingPrefix = 1784;
    pub const IMAGE_JXSS: EncodingPrefix = 1785;
    pub const IMAGE_KTX: EncodingPrefix = 1786;
    pub const IMAGE_KTX2: EncodingPrefix = 1787;
    pub const IMAGE_NAPLPS: EncodingPrefix = 1788;
    pub const IMAGE_PNG: EncodingPrefix = 1789;
    pub const IMAGE_PRS_BTIF: EncodingPrefix = 1790;
    pub const IMAGE_PRS_PTI: EncodingPrefix = 1791;
    pub const IMAGE_PWG_RASTER: EncodingPrefix = 1792;
    pub const IMAGE_SVG_XML: EncodingPrefix = 1793;
    pub const IMAGE_T38: EncodingPrefix = 1794;
    pub const IMAGE_TIFF: EncodingPrefix = 1795;
    pub const IMAGE_TIFF_FX: EncodingPrefix = 1796;
    pub const IMAGE_VND_ADOBE_PHOTOSHOP: EncodingPrefix = 1797;
    pub const IMAGE_VND_AIRZIP_ACCELERATOR_AZV: EncodingPrefix = 1798;
    pub const IMAGE_VND_CNS_INF2: EncodingPrefix = 1799;
    pub const IMAGE_VND_DECE_GRAPHIC: EncodingPrefix = 1800;
    pub const IMAGE_VND_DJVU: EncodingPrefix = 1801;
    pub const IMAGE_VND_DVB_SUBTITLE: EncodingPrefix = 1802;
    pub const IMAGE_VND_DWG: EncodingPrefix = 1803;
    pub const IMAGE_VND_DXF: EncodingPrefix = 1804;
    pub const IMAGE_VND_FASTBIDSHEET: EncodingPrefix = 1805;
    pub const IMAGE_VND_FPX: EncodingPrefix = 1806;
    pub const IMAGE_VND_FST: EncodingPrefix = 1807;
    pub const IMAGE_VND_FUJIXEROX_EDMICS_MMR: EncodingPrefix = 1808;
    pub const IMAGE_VND_FUJIXEROX_EDMICS_RLC: EncodingPrefix = 1809;
    pub const IMAGE_VND_GLOBALGRAPHICS_PGB: EncodingPrefix = 1810;
    pub const IMAGE_VND_MICROSOFT_ICON: EncodingPrefix = 1811;
    pub const IMAGE_VND_MIX: EncodingPrefix = 1812;
    pub const IMAGE_VND_MOZILLA_APNG: EncodingPrefix = 1813;
    pub const IMAGE_VND_MS_MODI: EncodingPrefix = 1814;
    pub const IMAGE_VND_NET_FPX: EncodingPrefix = 1815;
    pub const IMAGE_VND_PCO_B16: EncodingPrefix = 1816;
    pub const IMAGE_VND_RADIANCE: EncodingPrefix = 1817;
    pub const IMAGE_VND_SEALED_PNG: EncodingPrefix = 1818;
    pub const IMAGE_VND_SEALEDMEDIA_SOFTSEAL_GIF: EncodingPrefix = 1819;
    pub const IMAGE_VND_SEALEDMEDIA_SOFTSEAL_JPG: EncodingPrefix = 1820;
    pub const IMAGE_VND_SVF: EncodingPrefix = 1821;
    pub const IMAGE_VND_TENCENT_TAP: EncodingPrefix = 1822;
    pub const IMAGE_VND_VALVE_SOURCE_TEXTURE: EncodingPrefix = 1823;
    pub const IMAGE_VND_WAP_WBMP: EncodingPrefix = 1824;
    pub const IMAGE_VND_XIFF: EncodingPrefix = 1825;
    pub const IMAGE_VND_ZBRUSH_PCX: EncodingPrefix = 1826;
    pub const IMAGE_WEBP: EncodingPrefix = 1827;
    pub const IMAGE_WMF: EncodingPrefix = 1828;
    pub const MESSAGE_CPIM: EncodingPrefix = 1829;
    pub const MESSAGE_BHTTP: EncodingPrefix = 1830;
    pub const MESSAGE_DELIVERY_STATUS: EncodingPrefix = 1831;
    pub const MESSAGE_DISPOSITION_NOTIFICATION: EncodingPrefix = 1832;
    pub const MESSAGE_EXAMPLE: EncodingPrefix = 1833;
    pub const MESSAGE_EXTERNAL_BODY: EncodingPrefix = 1834;
    pub const MESSAGE_FEEDBACK_REPORT: EncodingPrefix = 1835;
    pub const MESSAGE_GLOBAL: EncodingPrefix = 1836;
    pub const MESSAGE_GLOBAL_DELIVERY_STATUS: EncodingPrefix = 1837;
    pub const MESSAGE_GLOBAL_DISPOSITION_NOTIFICATION: EncodingPrefix = 1838;
    pub const MESSAGE_GLOBAL_HEADERS: EncodingPrefix = 1839;
    pub const MESSAGE_HTTP: EncodingPrefix = 1840;
    pub const MESSAGE_IMDN_XML: EncodingPrefix = 1841;
    pub const MESSAGE_MLS: EncodingPrefix = 1842;
    pub const MESSAGE_NEWS: EncodingPrefix = 1843;
    pub const MESSAGE_OHTTP_REQ: EncodingPrefix = 1844;
    pub const MESSAGE_OHTTP_RES: EncodingPrefix = 1845;
    pub const MESSAGE_PARTIAL: EncodingPrefix = 1846;
    pub const MESSAGE_RFC822: EncodingPrefix = 1847;
    pub const MESSAGE_S_HTTP: EncodingPrefix = 1848;
    pub const MESSAGE_SIP: EncodingPrefix = 1849;
    pub const MESSAGE_SIPFRAG: EncodingPrefix = 1850;
    pub const MESSAGE_TRACKING_STATUS: EncodingPrefix = 1851;
    pub const MESSAGE_VND_SI_SIMP: EncodingPrefix = 1852;
    pub const MESSAGE_VND_WFA_WSC: EncodingPrefix = 1853;
    pub const MODEL_3MF: EncodingPrefix = 1854;
    pub const MODEL_JT: EncodingPrefix = 1855;
    pub const MODEL_E57: EncodingPrefix = 1856;
    pub const MODEL_EXAMPLE: EncodingPrefix = 1857;
    pub const MODEL_GLTF_JSON: EncodingPrefix = 1858;
    pub const MODEL_GLTF_BINARY: EncodingPrefix = 1859;
    pub const MODEL_IGES: EncodingPrefix = 1860;
    pub const MODEL_MESH: EncodingPrefix = 1861;
    pub const MODEL_MTL: EncodingPrefix = 1862;
    pub const MODEL_OBJ: EncodingPrefix = 1863;
    pub const MODEL_PRC: EncodingPrefix = 1864;
    pub const MODEL_STEP: EncodingPrefix = 1865;
    pub const MODEL_STEP_XML: EncodingPrefix = 1866;
    pub const MODEL_STEP_ZIP: EncodingPrefix = 1867;
    pub const MODEL_STEP_XML_ZIP: EncodingPrefix = 1868;
    pub const MODEL_STL: EncodingPrefix = 1869;
    pub const MODEL_U3D: EncodingPrefix = 1870;
    pub const MODEL_VND_BARY: EncodingPrefix = 1871;
    pub const MODEL_VND_CLD: EncodingPrefix = 1872;
    pub const MODEL_VND_COLLADA_XML: EncodingPrefix = 1873;
    pub const MODEL_VND_DWF: EncodingPrefix = 1874;
    pub const MODEL_VND_FLATLAND_3DML: EncodingPrefix = 1875;
    pub const MODEL_VND_GDL: EncodingPrefix = 1876;
    pub const MODEL_VND_GS_GDL: EncodingPrefix = 1877;
    pub const MODEL_VND_GTW: EncodingPrefix = 1878;
    pub const MODEL_VND_MOML_XML: EncodingPrefix = 1879;
    pub const MODEL_VND_MTS: EncodingPrefix = 1880;
    pub const MODEL_VND_OPENGEX: EncodingPrefix = 1881;
    pub const MODEL_VND_PARASOLID_TRANSMIT_BINARY: EncodingPrefix = 1882;
    pub const MODEL_VND_PARASOLID_TRANSMIT_TEXT: EncodingPrefix = 1883;
    pub const MODEL_VND_PYTHA_PYOX: EncodingPrefix = 1884;
    pub const MODEL_VND_ROSETTE_ANNOTATED_DATA_MODEL: EncodingPrefix = 1885;
    pub const MODEL_VND_SAP_VDS: EncodingPrefix = 1886;
    pub const MODEL_VND_USDA: EncodingPrefix = 1887;
    pub const MODEL_VND_USDZ_ZIP: EncodingPrefix = 1888;
    pub const MODEL_VND_VALVE_SOURCE_COMPILED_MAP: EncodingPrefix = 1889;
    pub const MODEL_VND_VTU: EncodingPrefix = 1890;
    pub const MODEL_VRML: EncodingPrefix = 1891;
    pub const MODEL_X3D_FASTINFOSET: EncodingPrefix = 1892;
    pub const MODEL_X3D_XML: EncodingPrefix = 1893;
    pub const MODEL_X3D_VRML: EncodingPrefix = 1894;
    pub const MULTIPART_ALTERNATIVE: EncodingPrefix = 1895;
    pub const MULTIPART_APPLEDOUBLE: EncodingPrefix = 1896;
    pub const MULTIPART_BYTERANGES: EncodingPrefix = 1897;
    pub const MULTIPART_DIGEST: EncodingPrefix = 1898;
    pub const MULTIPART_ENCRYPTED: EncodingPrefix = 1899;
    pub const MULTIPART_EXAMPLE: EncodingPrefix = 1900;
    pub const MULTIPART_FORM_DATA: EncodingPrefix = 1901;
    pub const MULTIPART_HEADER_SET: EncodingPrefix = 1902;
    pub const MULTIPART_MIXED: EncodingPrefix = 1903;
    pub const MULTIPART_MULTILINGUAL: EncodingPrefix = 1904;
    pub const MULTIPART_PARALLEL: EncodingPrefix = 1905;
    pub const MULTIPART_RELATED: EncodingPrefix = 1906;
    pub const MULTIPART_REPORT: EncodingPrefix = 1907;
    pub const MULTIPART_SIGNED: EncodingPrefix = 1908;
    pub const MULTIPART_VND_BINT_MED_PLUS: EncodingPrefix = 1909;
    pub const MULTIPART_VOICE_MESSAGE: EncodingPrefix = 1910;
    pub const MULTIPART_X_MIXED_REPLACE: EncodingPrefix = 1911;
    pub const TEXT_1D_INTERLEAVED_PARITYFEC: EncodingPrefix = 1912;
    pub const TEXT_RED: EncodingPrefix = 1913;
    pub const TEXT_SGML: EncodingPrefix = 1914;
    pub const TEXT_CACHE_MANIFEST: EncodingPrefix = 1915;
    pub const TEXT_CALENDAR: EncodingPrefix = 1916;
    pub const TEXT_CQL: EncodingPrefix = 1917;
    pub const TEXT_CQL_EXPRESSION: EncodingPrefix = 1918;
    pub const TEXT_CQL_IDENTIFIER: EncodingPrefix = 1919;
    pub const TEXT_CSS: EncodingPrefix = 1920;
    pub const TEXT_CSV: EncodingPrefix = 1921;
    pub const TEXT_CSV_SCHEMA: EncodingPrefix = 1922;
    pub const TEXT_DIRECTORY: EncodingPrefix = 1923;
    pub const TEXT_DNS: EncodingPrefix = 1924;
    pub const TEXT_ECMASCRIPT: EncodingPrefix = 1925;
    pub const TEXT_ENCAPRTP: EncodingPrefix = 1926;
    pub const TEXT_ENRICHED: EncodingPrefix = 1927;
    pub const TEXT_EXAMPLE: EncodingPrefix = 1928;
    pub const TEXT_FHIRPATH: EncodingPrefix = 1929;
    pub const TEXT_FLEXFEC: EncodingPrefix = 1930;
    pub const TEXT_FWDRED: EncodingPrefix = 1931;
    pub const TEXT_GFF3: EncodingPrefix = 1932;
    pub const TEXT_GRAMMAR_REF_LIST: EncodingPrefix = 1933;
    pub const TEXT_HL7V2: EncodingPrefix = 1934;
    pub const TEXT_HTML: EncodingPrefix = 1935;
    pub const TEXT_JAVASCRIPT: EncodingPrefix = 1936;
    pub const TEXT_JCR_CND: EncodingPrefix = 1937;
    pub const TEXT_MARKDOWN: EncodingPrefix = 1938;
    pub const TEXT_MIZAR: EncodingPrefix = 1939;
    pub const TEXT_N3: EncodingPrefix = 1940;
    pub const TEXT_PARAMETERS: EncodingPrefix = 1941;
    pub const TEXT_PARITYFEC: EncodingPrefix = 1942;
    pub const TEXT_PLAIN: EncodingPrefix = 1943;
    pub const TEXT_PROVENANCE_NOTATION: EncodingPrefix = 1944;
    pub const TEXT_PRS_FALLENSTEIN_RST: EncodingPrefix = 1945;
    pub const TEXT_PRS_LINES_TAG: EncodingPrefix = 1946;
    pub const TEXT_PRS_PROP_LOGIC: EncodingPrefix = 1947;
    pub const TEXT_PRS_TEXI: EncodingPrefix = 1948;
    pub const TEXT_RAPTORFEC: EncodingPrefix = 1949;
    pub const TEXT_RFC822_HEADERS: EncodingPrefix = 1950;
    pub const TEXT_RICHTEXT: EncodingPrefix = 1951;
    pub const TEXT_RTF: EncodingPrefix = 1952;
    pub const TEXT_RTP_ENC_AESCM128: EncodingPrefix = 1953;
    pub const TEXT_RTPLOOPBACK: EncodingPrefix = 1954;
    pub const TEXT_RTX: EncodingPrefix = 1955;
    pub const TEXT_SHACLC: EncodingPrefix = 1956;
    pub const TEXT_SHEX: EncodingPrefix = 1957;
    pub const TEXT_SPDX: EncodingPrefix = 1958;
    pub const TEXT_STRINGS: EncodingPrefix = 1959;
    pub const TEXT_T140: EncodingPrefix = 1960;
    pub const TEXT_TAB_SEPARATED_VALUES: EncodingPrefix = 1961;
    pub const TEXT_TROFF: EncodingPrefix = 1962;
    pub const TEXT_TURTLE: EncodingPrefix = 1963;
    pub const TEXT_ULPFEC: EncodingPrefix = 1964;
    pub const TEXT_URI_LIST: EncodingPrefix = 1965;
    pub const TEXT_VCARD: EncodingPrefix = 1966;
    pub const TEXT_VND_DMCLIENTSCRIPT: EncodingPrefix = 1967;
    pub const TEXT_VND_IPTC_NITF: EncodingPrefix = 1968;
    pub const TEXT_VND_IPTC_NEWSML: EncodingPrefix = 1969;
    pub const TEXT_VND_A: EncodingPrefix = 1970;
    pub const TEXT_VND_ABC: EncodingPrefix = 1971;
    pub const TEXT_VND_ASCII_ART: EncodingPrefix = 1972;
    pub const TEXT_VND_CURL: EncodingPrefix = 1973;
    pub const TEXT_VND_DEBIAN_COPYRIGHT: EncodingPrefix = 1974;
    pub const TEXT_VND_DVB_SUBTITLE: EncodingPrefix = 1975;
    pub const TEXT_VND_ESMERTEC_THEME_DESCRIPTOR: EncodingPrefix = 1976;
    pub const TEXT_VND_EXCHANGEABLE: EncodingPrefix = 1977;
    pub const TEXT_VND_FAMILYSEARCH_GEDCOM: EncodingPrefix = 1978;
    pub const TEXT_VND_FICLAB_FLT: EncodingPrefix = 1979;
    pub const TEXT_VND_FLY: EncodingPrefix = 1980;
    pub const TEXT_VND_FMI_FLEXSTOR: EncodingPrefix = 1981;
    pub const TEXT_VND_GML: EncodingPrefix = 1982;
    pub const TEXT_VND_GRAPHVIZ: EncodingPrefix = 1983;
    pub const TEXT_VND_HANS: EncodingPrefix = 1984;
    pub const TEXT_VND_HGL: EncodingPrefix = 1985;
    pub const TEXT_VND_IN3D_3DML: EncodingPrefix = 1986;
    pub const TEXT_VND_IN3D_SPOT: EncodingPrefix = 1987;
    pub const TEXT_VND_LATEX_Z: EncodingPrefix = 1988;
    pub const TEXT_VND_MOTOROLA_REFLEX: EncodingPrefix = 1989;
    pub const TEXT_VND_MS_MEDIAPACKAGE: EncodingPrefix = 1990;
    pub const TEXT_VND_NET2PHONE_COMMCENTER_COMMAND: EncodingPrefix = 1991;
    pub const TEXT_VND_RADISYS_MSML_BASIC_LAYOUT: EncodingPrefix = 1992;
    pub const TEXT_VND_SENX_WARPSCRIPT: EncodingPrefix = 1993;
    pub const TEXT_VND_SI_URICATALOGUE: EncodingPrefix = 1994;
    pub const TEXT_VND_SOSI: EncodingPrefix = 1995;
    pub const TEXT_VND_SUN_J2ME_APP_DESCRIPTOR: EncodingPrefix = 1996;
    pub const TEXT_VND_TROLLTECH_LINGUIST: EncodingPrefix = 1997;
    pub const TEXT_VND_WAP_SI: EncodingPrefix = 1998;
    pub const TEXT_VND_WAP_SL: EncodingPrefix = 1999;
    pub const TEXT_VND_WAP_WML: EncodingPrefix = 2000;
    pub const TEXT_VND_WAP_WMLSCRIPT: EncodingPrefix = 2001;
    pub const TEXT_VTT: EncodingPrefix = 2002;
    pub const TEXT_WGSL: EncodingPrefix = 2003;
    pub const TEXT_XML: EncodingPrefix = 2004;
    pub const TEXT_XML_EXTERNAL_PARSED_ENTITY: EncodingPrefix = 2005;
    pub const VIDEO_1D_INTERLEAVED_PARITYFEC: EncodingPrefix = 2006;
    pub const VIDEO_3GPP: EncodingPrefix = 2007;
    pub const VIDEO_3GPP_TT: EncodingPrefix = 2008;
    pub const VIDEO_3GPP2: EncodingPrefix = 2009;
    pub const VIDEO_AV1: EncodingPrefix = 2010;
    pub const VIDEO_BMPEG: EncodingPrefix = 2011;
    pub const VIDEO_BT656: EncodingPrefix = 2012;
    pub const VIDEO_CELB: EncodingPrefix = 2013;
    pub const VIDEO_DV: EncodingPrefix = 2014;
    pub const VIDEO_FFV1: EncodingPrefix = 2015;
    pub const VIDEO_H261: EncodingPrefix = 2016;
    pub const VIDEO_H263: EncodingPrefix = 2017;
    pub const VIDEO_H263_1998: EncodingPrefix = 2018;
    pub const VIDEO_H263_2000: EncodingPrefix = 2019;
    pub const VIDEO_H264: EncodingPrefix = 2020;
    pub const VIDEO_H264_RCDO: EncodingPrefix = 2021;
    pub const VIDEO_H264_SVC: EncodingPrefix = 2022;
    pub const VIDEO_H265: EncodingPrefix = 2023;
    pub const VIDEO_H266: EncodingPrefix = 2024;
    pub const VIDEO_JPEG: EncodingPrefix = 2025;
    pub const VIDEO_MP1S: EncodingPrefix = 2026;
    pub const VIDEO_MP2P: EncodingPrefix = 2027;
    pub const VIDEO_MP2T: EncodingPrefix = 2028;
    pub const VIDEO_MP4V_ES: EncodingPrefix = 2029;
    pub const VIDEO_MPV: EncodingPrefix = 2030;
    pub const VIDEO_SMPTE292M: EncodingPrefix = 2031;
    pub const VIDEO_VP8: EncodingPrefix = 2032;
    pub const VIDEO_VP9: EncodingPrefix = 2033;
    pub const VIDEO_ENCAPRTP: EncodingPrefix = 2034;
    pub const VIDEO_EVC: EncodingPrefix = 2035;
    pub const VIDEO_EXAMPLE: EncodingPrefix = 2036;
    pub const VIDEO_FLEXFEC: EncodingPrefix = 2037;
    pub const VIDEO_ISO_SEGMENT: EncodingPrefix = 2038;
    pub const VIDEO_JPEG2000: EncodingPrefix = 2039;
    pub const VIDEO_JXSV: EncodingPrefix = 2040;
    pub const VIDEO_MATROSKA: EncodingPrefix = 2041;
    pub const VIDEO_MATROSKA_3D: EncodingPrefix = 2042;
    pub const VIDEO_MJ2: EncodingPrefix = 2043;
    pub const VIDEO_MP4: EncodingPrefix = 2044;
    pub const VIDEO_MPEG: EncodingPrefix = 2045;
    pub const VIDEO_MPEG4_GENERIC: EncodingPrefix = 2046;
    pub const VIDEO_NV: EncodingPrefix = 2047;
    pub const VIDEO_OGG: EncodingPrefix = 2048;
    pub const VIDEO_PARITYFEC: EncodingPrefix = 2049;
    pub const VIDEO_POINTER: EncodingPrefix = 2050;
    pub const VIDEO_QUICKTIME: EncodingPrefix = 2051;
    pub const VIDEO_RAPTORFEC: EncodingPrefix = 2052;
    pub const VIDEO_RAW: EncodingPrefix = 2053;
    pub const VIDEO_RTP_ENC_AESCM128: EncodingPrefix = 2054;
    pub const VIDEO_RTPLOOPBACK: EncodingPrefix = 2055;
    pub const VIDEO_RTX: EncodingPrefix = 2056;
    pub const VIDEO_SCIP: EncodingPrefix = 2057;
    pub const VIDEO_SMPTE291: EncodingPrefix = 2058;
    pub const VIDEO_ULPFEC: EncodingPrefix = 2059;
    pub const VIDEO_VC1: EncodingPrefix = 2060;
    pub const VIDEO_VC2: EncodingPrefix = 2061;
    pub const VIDEO_VND_CCTV: EncodingPrefix = 2062;
    pub const VIDEO_VND_DECE_HD: EncodingPrefix = 2063;
    pub const VIDEO_VND_DECE_MOBILE: EncodingPrefix = 2064;
    pub const VIDEO_VND_DECE_MP4: EncodingPrefix = 2065;
    pub const VIDEO_VND_DECE_PD: EncodingPrefix = 2066;
    pub const VIDEO_VND_DECE_SD: EncodingPrefix = 2067;
    pub const VIDEO_VND_DECE_VIDEO: EncodingPrefix = 2068;
    pub const VIDEO_VND_DIRECTV_MPEG: EncodingPrefix = 2069;
    pub const VIDEO_VND_DIRECTV_MPEG_TTS: EncodingPrefix = 2070;
    pub const VIDEO_VND_DLNA_MPEG_TTS: EncodingPrefix = 2071;
    pub const VIDEO_VND_DVB_FILE: EncodingPrefix = 2072;
    pub const VIDEO_VND_FVT: EncodingPrefix = 2073;
    pub const VIDEO_VND_HNS_VIDEO: EncodingPrefix = 2074;
    pub const VIDEO_VND_IPTVFORUM_1DPARITYFEC_1010: EncodingPrefix = 2075;
    pub const VIDEO_VND_IPTVFORUM_1DPARITYFEC_2005: EncodingPrefix = 2076;
    pub const VIDEO_VND_IPTVFORUM_2DPARITYFEC_1010: EncodingPrefix = 2077;
    pub const VIDEO_VND_IPTVFORUM_2DPARITYFEC_2005: EncodingPrefix = 2078;
    pub const VIDEO_VND_IPTVFORUM_TTSAVC: EncodingPrefix = 2079;
    pub const VIDEO_VND_IPTVFORUM_TTSMPEG2: EncodingPrefix = 2080;
    pub const VIDEO_VND_MOTOROLA_VIDEO: EncodingPrefix = 2081;
    pub const VIDEO_VND_MOTOROLA_VIDEOP: EncodingPrefix = 2082;
    pub const VIDEO_VND_MPEGURL: EncodingPrefix = 2083;
    pub const VIDEO_VND_MS_PLAYREADY_MEDIA_PYV: EncodingPrefix = 2084;
    pub const VIDEO_VND_NOKIA_INTERLEAVED_MULTIMEDIA: EncodingPrefix = 2085;
    pub const VIDEO_VND_NOKIA_MP4VR: EncodingPrefix = 2086;
    pub const VIDEO_VND_NOKIA_VIDEOVOIP: EncodingPrefix = 2087;
    pub const VIDEO_VND_OBJECTVIDEO: EncodingPrefix = 2088;
    pub const VIDEO_VND_RADGAMETTOOLS_BINK: EncodingPrefix = 2089;
    pub const VIDEO_VND_RADGAMETTOOLS_SMACKER: EncodingPrefix = 2090;
    pub const VIDEO_VND_SEALED_MPEG1: EncodingPrefix = 2091;
    pub const VIDEO_VND_SEALED_MPEG4: EncodingPrefix = 2092;
    pub const VIDEO_VND_SEALED_SWF: EncodingPrefix = 2093;
    pub const VIDEO_VND_SEALEDMEDIA_SOFTSEAL_MOV: EncodingPrefix = 2094;
    pub const VIDEO_VND_UVVU_MP4: EncodingPrefix = 2095;
    pub const VIDEO_VND_VIVO: EncodingPrefix = 2096;
    pub const VIDEO_VND_YOUTUBE_YT: EncodingPrefix = 2097;

    pub(super) const KNOWN_PREFIX: phf::OrderedMap<EncodingPrefix, &'static str> = phf_ordered_map! {
        0u16 => "empty",
        1u16 => "application/1d-interleaved-parityfec",
        2u16 => "application/3gpdash-qoe-report+xml",
        3u16 => "application/3gpp-ims+xml",
        4u16 => "application/3gppHal+json",
        5u16 => "application/3gppHalForms+json",
        6u16 => "application/A2L",
        7u16 => "application/AML",
        8u16 => "application/ATF",
        9u16 => "application/ATFX",
        10u16 => "application/ATXML",
        11u16 => "application/CALS-1840",
        12u16 => "application/CDFX+XML",
        13u16 => "application/CEA",
        14u16 => "application/CSTAdata+xml",
        15u16 => "application/DCD",
        16u16 => "application/DII",
        17u16 => "application/DIT",
        18u16 => "application/EDI-X12",
        19u16 => "application/EDI-consent",
        20u16 => "application/EDIFACT",
        21u16 => "application/EmergencyCallData.Comment+xml",
        22u16 => "application/EmergencyCallData.Control+xml",
        23u16 => "application/EmergencyCallData.DeviceInfo+xml",
        24u16 => "application/EmergencyCallData.LegacyESN+json",
        25u16 => "application/EmergencyCallData.ProviderInfo+xml",
        26u16 => "application/EmergencyCallData.ServiceInfo+xml",
        27u16 => "application/EmergencyCallData.SubscriberInfo+xml",
        28u16 => "application/EmergencyCallData.VEDS+xml",
        29u16 => "application/EmergencyCallData.cap+xml",
        30u16 => "application/EmergencyCallData.eCall.MSD",
        31u16 => "application/H224",
        32u16 => "application/IOTP",
        33u16 => "application/ISUP",
        34u16 => "application/LXF",
        35u16 => "application/MF4",
        36u16 => "application/ODA",
        37u16 => "application/ODX",
        38u16 => "application/PDX",
        39u16 => "application/QSIG",
        40u16 => "application/SGML",
        41u16 => "application/TETRA_ISI",
        42u16 => "application/ace+cbor",
        43u16 => "application/ace+json",
        44u16 => "application/activemessage",
        45u16 => "application/activity+json",
        46u16 => "application/aif+cbor",
        47u16 => "application/aif+json",
        48u16 => "application/alto-cdni+json",
        49u16 => "application/alto-cdnifilter+json",
        50u16 => "application/alto-costmap+json",
        51u16 => "application/alto-costmapfilter+json",
        52u16 => "application/alto-directory+json",
        53u16 => "application/alto-endpointcost+json",
        54u16 => "application/alto-endpointcostparams+json",
        55u16 => "application/alto-endpointprop+json",
        56u16 => "application/alto-endpointpropparams+json",
        57u16 => "application/alto-error+json",
        58u16 => "application/alto-networkmap+json",
        59u16 => "application/alto-networkmapfilter+json",
        60u16 => "application/alto-propmap+json",
        61u16 => "application/alto-propmapparams+json",
        62u16 => "application/alto-tips+json",
        63u16 => "application/alto-tipsparams+json",
        64u16 => "application/alto-updatestreamcontrol+json",
        65u16 => "application/alto-updatestreamparams+json",
        66u16 => "application/andrew-inset",
        67u16 => "application/applefile",
        68u16 => "application/at+jwt",
        69u16 => "application/atom+xml",
        70u16 => "application/atomcat+xml",
        71u16 => "application/atomdeleted+xml",
        72u16 => "application/atomicmail",
        73u16 => "application/atomsvc+xml",
        74u16 => "application/atsc-dwd+xml",
        75u16 => "application/atsc-dynamic-event-message",
        76u16 => "application/atsc-held+xml",
        77u16 => "application/atsc-rdt+json",
        78u16 => "application/atsc-rsat+xml",
        79u16 => "application/auth-policy+xml",
        80u16 => "application/automationml-aml+xml",
        81u16 => "application/automationml-amlx+zip",
        82u16 => "application/bacnet-xdd+zip",
        83u16 => "application/batch-SMTP",
        84u16 => "application/beep+xml",
        85u16 => "application/c2pa",
        86u16 => "application/calendar+json",
        87u16 => "application/calendar+xml",
        88u16 => "application/call-completion",
        89u16 => "application/captive+json",
        90u16 => "application/cbor",
        91u16 => "application/cbor-seq",
        92u16 => "application/cccex",
        93u16 => "application/ccmp+xml",
        94u16 => "application/ccxml+xml",
        95u16 => "application/cda+xml",
        96u16 => "application/cdmi-capability",
        97u16 => "application/cdmi-container",
        98u16 => "application/cdmi-domain",
        99u16 => "application/cdmi-object",
        100u16 => "application/cdmi-queue",
        101u16 => "application/cdni",
        102u16 => "application/cea-2018+xml",
        103u16 => "application/cellml+xml",
        104u16 => "application/cfw",
        105u16 => "application/cid-edhoc+cbor-seq",
        106u16 => "application/city+json",
        107u16 => "application/clr",
        108u16 => "application/clue+xml",
        109u16 => "application/clue_info+xml",
        110u16 => "application/cms",
        111u16 => "application/cnrp+xml",
        112u16 => "application/coap-group+json",
        113u16 => "application/coap-payload",
        114u16 => "application/commonground",
        115u16 => "application/concise-problem-details+cbor",
        116u16 => "application/conference-info+xml",
        117u16 => "application/cose",
        118u16 => "application/cose-key",
        119u16 => "application/cose-key-set",
        120u16 => "application/cose-x509",
        121u16 => "application/cpl+xml",
        122u16 => "application/csrattrs",
        123u16 => "application/csta+xml",
        124u16 => "application/csvm+json",
        125u16 => "application/cwl",
        126u16 => "application/cwl+json",
        127u16 => "application/cwt",
        128u16 => "application/cybercash",
        129u16 => "application/dash+xml",
        130u16 => "application/dash-patch+xml",
        131u16 => "application/dashdelta",
        132u16 => "application/davmount+xml",
        133u16 => "application/dca-rft",
        134u16 => "application/dec-dx",
        135u16 => "application/dialog-info+xml",
        136u16 => "application/dicom",
        137u16 => "application/dicom+json",
        138u16 => "application/dicom+xml",
        139u16 => "application/dns",
        140u16 => "application/dns+json",
        141u16 => "application/dns-message",
        142u16 => "application/dots+cbor",
        143u16 => "application/dpop+jwt",
        144u16 => "application/dskpp+xml",
        145u16 => "application/dssc+der",
        146u16 => "application/dssc+xml",
        147u16 => "application/dvcs",
        148u16 => "application/ecmascript",
        149u16 => "application/edhoc+cbor-seq",
        150u16 => "application/efi",
        151u16 => "application/elm+json",
        152u16 => "application/elm+xml",
        153u16 => "application/emma+xml",
        154u16 => "application/emotionml+xml",
        155u16 => "application/encaprtp",
        156u16 => "application/epp+xml",
        157u16 => "application/epub+zip",
        158u16 => "application/eshop",
        159u16 => "application/example",
        160u16 => "application/exi",
        161u16 => "application/expect-ct-report+json",
        162u16 => "application/express",
        163u16 => "application/fastinfoset",
        164u16 => "application/fastsoap",
        165u16 => "application/fdf",
        166u16 => "application/fdt+xml",
        167u16 => "application/fhir+json",
        168u16 => "application/fhir+xml",
        169u16 => "application/fits",
        170u16 => "application/flexfec",
        171u16 => "application/font-sfnt",
        172u16 => "application/font-tdpfr",
        173u16 => "application/font-woff",
        174u16 => "application/framework-attributes+xml",
        175u16 => "application/geo+json",
        176u16 => "application/geo+json-seq",
        177u16 => "application/geopackage+sqlite3",
        178u16 => "application/geoxacml+json",
        179u16 => "application/geoxacml+xml",
        180u16 => "application/gltf-buffer",
        181u16 => "application/gml+xml",
        182u16 => "application/gzip",
        183u16 => "application/held+xml",
        184u16 => "application/hl7v2+xml",
        185u16 => "application/http",
        186u16 => "application/hyperstudio",
        187u16 => "application/ibe-key-request+xml",
        188u16 => "application/ibe-pkg-reply+xml",
        189u16 => "application/ibe-pp-data",
        190u16 => "application/iges",
        191u16 => "application/im-iscomposing+xml",
        192u16 => "application/index",
        193u16 => "application/index.cmd",
        194u16 => "application/index.obj",
        195u16 => "application/index.response",
        196u16 => "application/index.vnd",
        197u16 => "application/inkml+xml",
        198u16 => "application/ipfix",
        199u16 => "application/ipp",
        200u16 => "application/its+xml",
        201u16 => "application/java-archive",
        202u16 => "application/javascript",
        203u16 => "application/jf2feed+json",
        204u16 => "application/jose",
        205u16 => "application/jose+json",
        206u16 => "application/jrd+json",
        207u16 => "application/jscalendar+json",
        208u16 => "application/jscontact+json",
        209u16 => "application/json",
        210u16 => "application/json-patch+json",
        211u16 => "application/json-seq",
        212u16 => "application/jsonpath",
        213u16 => "application/jwk+json",
        214u16 => "application/jwk-set+json",
        215u16 => "application/jwt",
        216u16 => "application/kpml-request+xml",
        217u16 => "application/kpml-response+xml",
        218u16 => "application/ld+json",
        219u16 => "application/lgr+xml",
        220u16 => "application/link-format",
        221u16 => "application/linkset",
        222u16 => "application/linkset+json",
        223u16 => "application/load-control+xml",
        224u16 => "application/logout+jwt",
        225u16 => "application/lost+xml",
        226u16 => "application/lostsync+xml",
        227u16 => "application/lpf+zip",
        228u16 => "application/mac-binhex40",
        229u16 => "application/macwriteii",
        230u16 => "application/mads+xml",
        231u16 => "application/manifest+json",
        232u16 => "application/marc",
        233u16 => "application/marcxml+xml",
        234u16 => "application/mathematica",
        235u16 => "application/mathml+xml",
        236u16 => "application/mathml-content+xml",
        237u16 => "application/mathml-presentation+xml",
        238u16 => "application/mbms-associated-procedure-description+xml",
        239u16 => "application/mbms-deregister+xml",
        240u16 => "application/mbms-envelope+xml",
        241u16 => "application/mbms-msk+xml",
        242u16 => "application/mbms-msk-response+xml",
        243u16 => "application/mbms-protection-description+xml",
        244u16 => "application/mbms-reception-report+xml",
        245u16 => "application/mbms-register+xml",
        246u16 => "application/mbms-register-response+xml",
        247u16 => "application/mbms-schedule+xml",
        248u16 => "application/mbms-user-service-description+xml",
        249u16 => "application/mbox",
        250u16 => "application/media-policy-dataset+xml",
        251u16 => "application/media_control+xml",
        252u16 => "application/mediaservercontrol+xml",
        253u16 => "application/merge-patch+json",
        254u16 => "application/metalink4+xml",
        255u16 => "application/mets+xml",
        256u16 => "application/mikey",
        257u16 => "application/mipc",
        258u16 => "application/missing-blocks+cbor-seq",
        259u16 => "application/mmt-aei+xml",
        260u16 => "application/mmt-usd+xml",
        261u16 => "application/mods+xml",
        262u16 => "application/moss-keys",
        263u16 => "application/moss-signature",
        264u16 => "application/mosskey-data",
        265u16 => "application/mosskey-request",
        266u16 => "application/mp21",
        267u16 => "application/mp4",
        268u16 => "application/mpeg4-generic",
        269u16 => "application/mpeg4-iod",
        270u16 => "application/mpeg4-iod-xmt",
        271u16 => "application/mrb-consumer+xml",
        272u16 => "application/mrb-publish+xml",
        273u16 => "application/msc-ivr+xml",
        274u16 => "application/msc-mixer+xml",
        275u16 => "application/msword",
        276u16 => "application/mud+json",
        277u16 => "application/multipart-core",
        278u16 => "application/mxf",
        279u16 => "application/n-quads",
        280u16 => "application/n-triples",
        281u16 => "application/nasdata",
        282u16 => "application/news-checkgroups",
        283u16 => "application/news-groupinfo",
        284u16 => "application/news-transmission",
        285u16 => "application/nlsml+xml",
        286u16 => "application/node",
        287u16 => "application/nss",
        288u16 => "application/oauth-authz-req+jwt",
        289u16 => "application/oblivious-dns-message",
        290u16 => "application/ocsp-request",
        291u16 => "application/ocsp-response",
        292u16 => "application/octet-stream",
        293u16 => "application/odm+xml",
        294u16 => "application/oebps-package+xml",
        295u16 => "application/ogg",
        296u16 => "application/ohttp-keys",
        297u16 => "application/opc-nodeset+xml",
        298u16 => "application/oscore",
        299u16 => "application/oxps",
        300u16 => "application/p21",
        301u16 => "application/p21+zip",
        302u16 => "application/p2p-overlay+xml",
        303u16 => "application/parityfec",
        304u16 => "application/passport",
        305u16 => "application/patch-ops-error+xml",
        306u16 => "application/pdf",
        307u16 => "application/pem-certificate-chain",
        308u16 => "application/pgp-encrypted",
        309u16 => "application/pgp-keys",
        310u16 => "application/pgp-signature",
        311u16 => "application/pidf+xml",
        312u16 => "application/pidf-diff+xml",
        313u16 => "application/pkcs10",
        314u16 => "application/pkcs12",
        315u16 => "application/pkcs7-mime",
        316u16 => "application/pkcs7-signature",
        317u16 => "application/pkcs8",
        318u16 => "application/pkcs8-encrypted",
        319u16 => "application/pkix-attr-cert",
        320u16 => "application/pkix-cert",
        321u16 => "application/pkix-crl",
        322u16 => "application/pkix-pkipath",
        323u16 => "application/pkixcmp",
        324u16 => "application/pls+xml",
        325u16 => "application/poc-settings+xml",
        326u16 => "application/postscript",
        327u16 => "application/ppsp-tracker+json",
        328u16 => "application/private-token-issuer-directory",
        329u16 => "application/private-token-request",
        330u16 => "application/private-token-response",
        331u16 => "application/problem+json",
        332u16 => "application/problem+xml",
        333u16 => "application/provenance+xml",
        334u16 => "application/prs.alvestrand.titrax-sheet",
        335u16 => "application/prs.cww",
        336u16 => "application/prs.cyn",
        337u16 => "application/prs.hpub+zip",
        338u16 => "application/prs.implied-document+xml",
        339u16 => "application/prs.implied-executable",
        340u16 => "application/prs.implied-object+json",
        341u16 => "application/prs.implied-object+json-seq",
        342u16 => "application/prs.implied-object+yaml",
        343u16 => "application/prs.implied-structure",
        344u16 => "application/prs.nprend",
        345u16 => "application/prs.plucker",
        346u16 => "application/prs.rdf-xml-crypt",
        347u16 => "application/prs.vcfbzip2",
        348u16 => "application/prs.xsf+xml",
        349u16 => "application/pskc+xml",
        350u16 => "application/pvd+json",
        351u16 => "application/raptorfec",
        352u16 => "application/rdap+json",
        353u16 => "application/rdf+xml",
        354u16 => "application/reginfo+xml",
        355u16 => "application/relax-ng-compact-syntax",
        356u16 => "application/remote-printing",
        357u16 => "application/reputon+json",
        358u16 => "application/resource-lists+xml",
        359u16 => "application/resource-lists-diff+xml",
        360u16 => "application/rfc+xml",
        361u16 => "application/riscos",
        362u16 => "application/rlmi+xml",
        363u16 => "application/rls-services+xml",
        364u16 => "application/route-apd+xml",
        365u16 => "application/route-s-tsid+xml",
        366u16 => "application/route-usd+xml",
        367u16 => "application/rpki-checklist",
        368u16 => "application/rpki-ghostbusters",
        369u16 => "application/rpki-manifest",
        370u16 => "application/rpki-publication",
        371u16 => "application/rpki-roa",
        372u16 => "application/rpki-updown",
        373u16 => "application/rtf",
        374u16 => "application/rtploopback",
        375u16 => "application/rtx",
        376u16 => "application/samlassertion+xml",
        377u16 => "application/samlmetadata+xml",
        378u16 => "application/sarif+json",
        379u16 => "application/sarif-external-properties+json",
        380u16 => "application/sbe",
        381u16 => "application/sbml+xml",
        382u16 => "application/scaip+xml",
        383u16 => "application/scim+json",
        384u16 => "application/scvp-cv-request",
        385u16 => "application/scvp-cv-response",
        386u16 => "application/scvp-vp-request",
        387u16 => "application/scvp-vp-response",
        388u16 => "application/sdp",
        389u16 => "application/secevent+jwt",
        390u16 => "application/senml+cbor",
        391u16 => "application/senml+json",
        392u16 => "application/senml+xml",
        393u16 => "application/senml-etch+cbor",
        394u16 => "application/senml-etch+json",
        395u16 => "application/senml-exi",
        396u16 => "application/sensml+cbor",
        397u16 => "application/sensml+json",
        398u16 => "application/sensml+xml",
        399u16 => "application/sensml-exi",
        400u16 => "application/sep+xml",
        401u16 => "application/sep-exi",
        402u16 => "application/session-info",
        403u16 => "application/set-payment",
        404u16 => "application/set-payment-initiation",
        405u16 => "application/set-registration",
        406u16 => "application/set-registration-initiation",
        407u16 => "application/sgml-open-catalog",
        408u16 => "application/shf+xml",
        409u16 => "application/sieve",
        410u16 => "application/simple-filter+xml",
        411u16 => "application/simple-message-summary",
        412u16 => "application/simpleSymbolContainer",
        413u16 => "application/sipc",
        414u16 => "application/slate",
        415u16 => "application/smil",
        416u16 => "application/smil+xml",
        417u16 => "application/smpte336m",
        418u16 => "application/soap+fastinfoset",
        419u16 => "application/soap+xml",
        420u16 => "application/sparql-query",
        421u16 => "application/sparql-results+xml",
        422u16 => "application/spdx+json",
        423u16 => "application/spirits-event+xml",
        424u16 => "application/sql",
        425u16 => "application/srgs",
        426u16 => "application/srgs+xml",
        427u16 => "application/sru+xml",
        428u16 => "application/ssml+xml",
        429u16 => "application/stix+json",
        430u16 => "application/swid+cbor",
        431u16 => "application/swid+xml",
        432u16 => "application/tamp-apex-update",
        433u16 => "application/tamp-apex-update-confirm",
        434u16 => "application/tamp-community-update",
        435u16 => "application/tamp-community-update-confirm",
        436u16 => "application/tamp-error",
        437u16 => "application/tamp-sequence-adjust",
        438u16 => "application/tamp-sequence-adjust-confirm",
        439u16 => "application/tamp-status-query",
        440u16 => "application/tamp-status-response",
        441u16 => "application/tamp-update",
        442u16 => "application/tamp-update-confirm",
        443u16 => "application/taxii+json",
        444u16 => "application/td+json",
        445u16 => "application/tei+xml",
        446u16 => "application/thraud+xml",
        447u16 => "application/timestamp-query",
        448u16 => "application/timestamp-reply",
        449u16 => "application/timestamped-data",
        450u16 => "application/tlsrpt+gzip",
        451u16 => "application/tlsrpt+json",
        452u16 => "application/tm+json",
        453u16 => "application/tnauthlist",
        454u16 => "application/token-introspection+jwt",
        455u16 => "application/trickle-ice-sdpfrag",
        456u16 => "application/trig",
        457u16 => "application/ttml+xml",
        458u16 => "application/tve-trigger",
        459u16 => "application/tzif",
        460u16 => "application/tzif-leap",
        461u16 => "application/ulpfec",
        462u16 => "application/urc-grpsheet+xml",
        463u16 => "application/urc-ressheet+xml",
        464u16 => "application/urc-targetdesc+xml",
        465u16 => "application/urc-uisocketdesc+xml",
        466u16 => "application/vcard+json",
        467u16 => "application/vcard+xml",
        468u16 => "application/vemmi",
        469u16 => "application/vnd.1000minds.decision-model+xml",
        470u16 => "application/vnd.1ob",
        471u16 => "application/vnd.3M.Post-it-Notes",
        472u16 => "application/vnd.3gpp-prose+xml",
        473u16 => "application/vnd.3gpp-prose-pc3a+xml",
        474u16 => "application/vnd.3gpp-prose-pc3ach+xml",
        475u16 => "application/vnd.3gpp-prose-pc3ch+xml",
        476u16 => "application/vnd.3gpp-prose-pc8+xml",
        477u16 => "application/vnd.3gpp-v2x-local-service-information",
        478u16 => "application/vnd.3gpp.5gnas",
        479u16 => "application/vnd.3gpp.GMOP+xml",
        480u16 => "application/vnd.3gpp.SRVCC-info+xml",
        481u16 => "application/vnd.3gpp.access-transfer-events+xml",
        482u16 => "application/vnd.3gpp.bsf+xml",
        483u16 => "application/vnd.3gpp.crs+xml",
        484u16 => "application/vnd.3gpp.current-location-discovery+xml",
        485u16 => "application/vnd.3gpp.gtpc",
        486u16 => "application/vnd.3gpp.interworking-data",
        487u16 => "application/vnd.3gpp.lpp",
        488u16 => "application/vnd.3gpp.mc-signalling-ear",
        489u16 => "application/vnd.3gpp.mcdata-affiliation-command+xml",
        490u16 => "application/vnd.3gpp.mcdata-info+xml",
        491u16 => "application/vnd.3gpp.mcdata-msgstore-ctrl-request+xml",
        492u16 => "application/vnd.3gpp.mcdata-payload",
        493u16 => "application/vnd.3gpp.mcdata-regroup+xml",
        494u16 => "application/vnd.3gpp.mcdata-service-config+xml",
        495u16 => "application/vnd.3gpp.mcdata-signalling",
        496u16 => "application/vnd.3gpp.mcdata-ue-config+xml",
        497u16 => "application/vnd.3gpp.mcdata-user-profile+xml",
        498u16 => "application/vnd.3gpp.mcptt-affiliation-command+xml",
        499u16 => "application/vnd.3gpp.mcptt-floor-request+xml",
        500u16 => "application/vnd.3gpp.mcptt-info+xml",
        501u16 => "application/vnd.3gpp.mcptt-location-info+xml",
        502u16 => "application/vnd.3gpp.mcptt-mbms-usage-info+xml",
        503u16 => "application/vnd.3gpp.mcptt-regroup+xml",
        504u16 => "application/vnd.3gpp.mcptt-service-config+xml",
        505u16 => "application/vnd.3gpp.mcptt-signed+xml",
        506u16 => "application/vnd.3gpp.mcptt-ue-config+xml",
        507u16 => "application/vnd.3gpp.mcptt-ue-init-config+xml",
        508u16 => "application/vnd.3gpp.mcptt-user-profile+xml",
        509u16 => "application/vnd.3gpp.mcvideo-affiliation-command+xml",
        510u16 => "application/vnd.3gpp.mcvideo-affiliation-info+xml",
        511u16 => "application/vnd.3gpp.mcvideo-info+xml",
        512u16 => "application/vnd.3gpp.mcvideo-location-info+xml",
        513u16 => "application/vnd.3gpp.mcvideo-mbms-usage-info+xml",
        514u16 => "application/vnd.3gpp.mcvideo-regroup+xml",
        515u16 => "application/vnd.3gpp.mcvideo-service-config+xml",
        516u16 => "application/vnd.3gpp.mcvideo-transmission-request+xml",
        517u16 => "application/vnd.3gpp.mcvideo-ue-config+xml",
        518u16 => "application/vnd.3gpp.mcvideo-user-profile+xml",
        519u16 => "application/vnd.3gpp.mid-call+xml",
        520u16 => "application/vnd.3gpp.ngap",
        521u16 => "application/vnd.3gpp.pfcp",
        522u16 => "application/vnd.3gpp.pic-bw-large",
        523u16 => "application/vnd.3gpp.pic-bw-small",
        524u16 => "application/vnd.3gpp.pic-bw-var",
        525u16 => "application/vnd.3gpp.s1ap",
        526u16 => "application/vnd.3gpp.seal-group-doc+xml",
        527u16 => "application/vnd.3gpp.seal-info+xml",
        528u16 => "application/vnd.3gpp.seal-location-info+xml",
        529u16 => "application/vnd.3gpp.seal-mbms-usage-info+xml",
        530u16 => "application/vnd.3gpp.seal-network-QoS-management-info+xml",
        531u16 => "application/vnd.3gpp.seal-ue-config-info+xml",
        532u16 => "application/vnd.3gpp.seal-unicast-info+xml",
        533u16 => "application/vnd.3gpp.seal-user-profile-info+xml",
        534u16 => "application/vnd.3gpp.sms",
        535u16 => "application/vnd.3gpp.sms+xml",
        536u16 => "application/vnd.3gpp.srvcc-ext+xml",
        537u16 => "application/vnd.3gpp.state-and-event-info+xml",
        538u16 => "application/vnd.3gpp.ussd+xml",
        539u16 => "application/vnd.3gpp.v2x",
        540u16 => "application/vnd.3gpp.vae-info+xml",
        541u16 => "application/vnd.3gpp2.bcmcsinfo+xml",
        542u16 => "application/vnd.3gpp2.sms",
        543u16 => "application/vnd.3gpp2.tcap",
        544u16 => "application/vnd.3lightssoftware.imagescal",
        545u16 => "application/vnd.FloGraphIt",
        546u16 => "application/vnd.HandHeld-Entertainment+xml",
        547u16 => "application/vnd.Kinar",
        548u16 => "application/vnd.MFER",
        549u16 => "application/vnd.Mobius.DAF",
        550u16 => "application/vnd.Mobius.DIS",
        551u16 => "application/vnd.Mobius.MBK",
        552u16 => "application/vnd.Mobius.MQY",
        553u16 => "application/vnd.Mobius.MSL",
        554u16 => "application/vnd.Mobius.PLC",
        555u16 => "application/vnd.Mobius.TXF",
        556u16 => "application/vnd.Quark.QuarkXPress",
        557u16 => "application/vnd.RenLearn.rlprint",
        558u16 => "application/vnd.SimTech-MindMapper",
        559u16 => "application/vnd.accpac.simply.aso",
        560u16 => "application/vnd.accpac.simply.imp",
        561u16 => "application/vnd.acm.addressxfer+json",
        562u16 => "application/vnd.acm.chatbot+json",
        563u16 => "application/vnd.acucobol",
        564u16 => "application/vnd.acucorp",
        565u16 => "application/vnd.adobe.flash.movie",
        566u16 => "application/vnd.adobe.formscentral.fcdt",
        567u16 => "application/vnd.adobe.fxp",
        568u16 => "application/vnd.adobe.partial-upload",
        569u16 => "application/vnd.adobe.xdp+xml",
        570u16 => "application/vnd.aether.imp",
        571u16 => "application/vnd.afpc.afplinedata",
        572u16 => "application/vnd.afpc.afplinedata-pagedef",
        573u16 => "application/vnd.afpc.cmoca-cmresource",
        574u16 => "application/vnd.afpc.foca-charset",
        575u16 => "application/vnd.afpc.foca-codedfont",
        576u16 => "application/vnd.afpc.foca-codepage",
        577u16 => "application/vnd.afpc.modca",
        578u16 => "application/vnd.afpc.modca-cmtable",
        579u16 => "application/vnd.afpc.modca-formdef",
        580u16 => "application/vnd.afpc.modca-mediummap",
        581u16 => "application/vnd.afpc.modca-objectcontainer",
        582u16 => "application/vnd.afpc.modca-overlay",
        583u16 => "application/vnd.afpc.modca-pagesegment",
        584u16 => "application/vnd.age",
        585u16 => "application/vnd.ah-barcode",
        586u16 => "application/vnd.ahead.space",
        587u16 => "application/vnd.airzip.filesecure.azf",
        588u16 => "application/vnd.airzip.filesecure.azs",
        589u16 => "application/vnd.amadeus+json",
        590u16 => "application/vnd.amazon.mobi8-ebook",
        591u16 => "application/vnd.americandynamics.acc",
        592u16 => "application/vnd.amiga.ami",
        593u16 => "application/vnd.amundsen.maze+xml",
        594u16 => "application/vnd.android.ota",
        595u16 => "application/vnd.anki",
        596u16 => "application/vnd.anser-web-certificate-issue-initiation",
        597u16 => "application/vnd.antix.game-component",
        598u16 => "application/vnd.apache.arrow.file",
        599u16 => "application/vnd.apache.arrow.stream",
        600u16 => "application/vnd.apache.parquet",
        601u16 => "application/vnd.apache.thrift.binary",
        602u16 => "application/vnd.apache.thrift.compact",
        603u16 => "application/vnd.apache.thrift.json",
        604u16 => "application/vnd.apexlang",
        605u16 => "application/vnd.api+json",
        606u16 => "application/vnd.aplextor.warrp+json",
        607u16 => "application/vnd.apothekende.reservation+json",
        608u16 => "application/vnd.apple.installer+xml",
        609u16 => "application/vnd.apple.keynote",
        610u16 => "application/vnd.apple.mpegurl",
        611u16 => "application/vnd.apple.numbers",
        612u16 => "application/vnd.apple.pages",
        613u16 => "application/vnd.arastra.swi",
        614u16 => "application/vnd.aristanetworks.swi",
        615u16 => "application/vnd.artisan+json",
        616u16 => "application/vnd.artsquare",
        617u16 => "application/vnd.astraea-software.iota",
        618u16 => "application/vnd.audiograph",
        619u16 => "application/vnd.autopackage",
        620u16 => "application/vnd.avalon+json",
        621u16 => "application/vnd.avistar+xml",
        622u16 => "application/vnd.balsamiq.bmml+xml",
        623u16 => "application/vnd.balsamiq.bmpr",
        624u16 => "application/vnd.banana-accounting",
        625u16 => "application/vnd.bbf.usp.error",
        626u16 => "application/vnd.bbf.usp.msg",
        627u16 => "application/vnd.bbf.usp.msg+json",
        628u16 => "application/vnd.bekitzur-stech+json",
        629u16 => "application/vnd.belightsoft.lhzd+zip",
        630u16 => "application/vnd.belightsoft.lhzl+zip",
        631u16 => "application/vnd.bint.med-content",
        632u16 => "application/vnd.biopax.rdf+xml",
        633u16 => "application/vnd.blink-idb-value-wrapper",
        634u16 => "application/vnd.blueice.multipass",
        635u16 => "application/vnd.bluetooth.ep.oob",
        636u16 => "application/vnd.bluetooth.le.oob",
        637u16 => "application/vnd.bmi",
        638u16 => "application/vnd.bpf",
        639u16 => "application/vnd.bpf3",
        640u16 => "application/vnd.businessobjects",
        641u16 => "application/vnd.byu.uapi+json",
        642u16 => "application/vnd.bzip3",
        643u16 => "application/vnd.cab-jscript",
        644u16 => "application/vnd.canon-cpdl",
        645u16 => "application/vnd.canon-lips",
        646u16 => "application/vnd.capasystems-pg+json",
        647u16 => "application/vnd.cendio.thinlinc.clientconf",
        648u16 => "application/vnd.century-systems.tcp_stream",
        649u16 => "application/vnd.chemdraw+xml",
        650u16 => "application/vnd.chess-pgn",
        651u16 => "application/vnd.chipnuts.karaoke-mmd",
        652u16 => "application/vnd.ciedi",
        653u16 => "application/vnd.cinderella",
        654u16 => "application/vnd.cirpack.isdn-ext",
        655u16 => "application/vnd.citationstyles.style+xml",
        656u16 => "application/vnd.claymore",
        657u16 => "application/vnd.cloanto.rp9",
        658u16 => "application/vnd.clonk.c4group",
        659u16 => "application/vnd.cluetrust.cartomobile-config",
        660u16 => "application/vnd.cluetrust.cartomobile-config-pkg",
        661u16 => "application/vnd.cncf.helm.chart.content.v1.tar+gzip",
        662u16 => "application/vnd.cncf.helm.chart.provenance.v1.prov",
        663u16 => "application/vnd.cncf.helm.config.v1+json",
        664u16 => "application/vnd.coffeescript",
        665u16 => "application/vnd.collabio.xodocuments.document",
        666u16 => "application/vnd.collabio.xodocuments.document-template",
        667u16 => "application/vnd.collabio.xodocuments.presentation",
        668u16 => "application/vnd.collabio.xodocuments.presentation-template",
        669u16 => "application/vnd.collabio.xodocuments.spreadsheet",
        670u16 => "application/vnd.collabio.xodocuments.spreadsheet-template",
        671u16 => "application/vnd.collection+json",
        672u16 => "application/vnd.collection.doc+json",
        673u16 => "application/vnd.collection.next+json",
        674u16 => "application/vnd.comicbook+zip",
        675u16 => "application/vnd.comicbook-rar",
        676u16 => "application/vnd.commerce-battelle",
        677u16 => "application/vnd.commonspace",
        678u16 => "application/vnd.contact.cmsg",
        679u16 => "application/vnd.coreos.ignition+json",
        680u16 => "application/vnd.cosmocaller",
        681u16 => "application/vnd.crick.clicker",
        682u16 => "application/vnd.crick.clicker.keyboard",
        683u16 => "application/vnd.crick.clicker.palette",
        684u16 => "application/vnd.crick.clicker.template",
        685u16 => "application/vnd.crick.clicker.wordbank",
        686u16 => "application/vnd.criticaltools.wbs+xml",
        687u16 => "application/vnd.cryptii.pipe+json",
        688u16 => "application/vnd.crypto-shade-file",
        689u16 => "application/vnd.cryptomator.encrypted",
        690u16 => "application/vnd.cryptomator.vault",
        691u16 => "application/vnd.ctc-posml",
        692u16 => "application/vnd.ctct.ws+xml",
        693u16 => "application/vnd.cups-pdf",
        694u16 => "application/vnd.cups-postscript",
        695u16 => "application/vnd.cups-ppd",
        696u16 => "application/vnd.cups-raster",
        697u16 => "application/vnd.cups-raw",
        698u16 => "application/vnd.curl",
        699u16 => "application/vnd.cyan.dean.root+xml",
        700u16 => "application/vnd.cybank",
        701u16 => "application/vnd.cyclonedx+json",
        702u16 => "application/vnd.cyclonedx+xml",
        703u16 => "application/vnd.d2l.coursepackage1p0+zip",
        704u16 => "application/vnd.d3m-dataset",
        705u16 => "application/vnd.d3m-problem",
        706u16 => "application/vnd.dart",
        707u16 => "application/vnd.data-vision.rdz",
        708u16 => "application/vnd.datalog",
        709u16 => "application/vnd.datapackage+json",
        710u16 => "application/vnd.dataresource+json",
        711u16 => "application/vnd.dbf",
        712u16 => "application/vnd.debian.binary-package",
        713u16 => "application/vnd.dece.data",
        714u16 => "application/vnd.dece.ttml+xml",
        715u16 => "application/vnd.dece.unspecified",
        716u16 => "application/vnd.dece.zip",
        717u16 => "application/vnd.denovo.fcselayout-link",
        718u16 => "application/vnd.desmume.movie",
        719u16 => "application/vnd.dir-bi.plate-dl-nosuffix",
        720u16 => "application/vnd.dm.delegation+xml",
        721u16 => "application/vnd.dna",
        722u16 => "application/vnd.document+json",
        723u16 => "application/vnd.dolby.mobile.1",
        724u16 => "application/vnd.dolby.mobile.2",
        725u16 => "application/vnd.doremir.scorecloud-binary-document",
        726u16 => "application/vnd.dpgraph",
        727u16 => "application/vnd.dreamfactory",
        728u16 => "application/vnd.drive+json",
        729u16 => "application/vnd.dtg.local",
        730u16 => "application/vnd.dtg.local.flash",
        731u16 => "application/vnd.dtg.local.html",
        732u16 => "application/vnd.dvb.ait",
        733u16 => "application/vnd.dvb.dvbisl+xml",
        734u16 => "application/vnd.dvb.dvbj",
        735u16 => "application/vnd.dvb.esgcontainer",
        736u16 => "application/vnd.dvb.ipdcdftnotifaccess",
        737u16 => "application/vnd.dvb.ipdcesgaccess",
        738u16 => "application/vnd.dvb.ipdcesgaccess2",
        739u16 => "application/vnd.dvb.ipdcesgpdd",
        740u16 => "application/vnd.dvb.ipdcroaming",
        741u16 => "application/vnd.dvb.iptv.alfec-base",
        742u16 => "application/vnd.dvb.iptv.alfec-enhancement",
        743u16 => "application/vnd.dvb.notif-aggregate-root+xml",
        744u16 => "application/vnd.dvb.notif-container+xml",
        745u16 => "application/vnd.dvb.notif-generic+xml",
        746u16 => "application/vnd.dvb.notif-ia-msglist+xml",
        747u16 => "application/vnd.dvb.notif-ia-registration-request+xml",
        748u16 => "application/vnd.dvb.notif-ia-registration-response+xml",
        749u16 => "application/vnd.dvb.notif-init+xml",
        750u16 => "application/vnd.dvb.pfr",
        751u16 => "application/vnd.dvb.service",
        752u16 => "application/vnd.dxr",
        753u16 => "application/vnd.dynageo",
        754u16 => "application/vnd.dzr",
        755u16 => "application/vnd.easykaraoke.cdgdownload",
        756u16 => "application/vnd.ecdis-update",
        757u16 => "application/vnd.ecip.rlp",
        758u16 => "application/vnd.eclipse.ditto+json",
        759u16 => "application/vnd.ecowin.chart",
        760u16 => "application/vnd.ecowin.filerequest",
        761u16 => "application/vnd.ecowin.fileupdate",
        762u16 => "application/vnd.ecowin.series",
        763u16 => "application/vnd.ecowin.seriesrequest",
        764u16 => "application/vnd.ecowin.seriesupdate",
        765u16 => "application/vnd.efi.img",
        766u16 => "application/vnd.efi.iso",
        767u16 => "application/vnd.eln+zip",
        768u16 => "application/vnd.emclient.accessrequest+xml",
        769u16 => "application/vnd.enliven",
        770u16 => "application/vnd.enphase.envoy",
        771u16 => "application/vnd.eprints.data+xml",
        772u16 => "application/vnd.epson.esf",
        773u16 => "application/vnd.epson.msf",
        774u16 => "application/vnd.epson.quickanime",
        775u16 => "application/vnd.epson.salt",
        776u16 => "application/vnd.epson.ssf",
        777u16 => "application/vnd.ericsson.quickcall",
        778u16 => "application/vnd.erofs",
        779u16 => "application/vnd.espass-espass+zip",
        780u16 => "application/vnd.eszigno3+xml",
        781u16 => "application/vnd.etsi.aoc+xml",
        782u16 => "application/vnd.etsi.asic-e+zip",
        783u16 => "application/vnd.etsi.asic-s+zip",
        784u16 => "application/vnd.etsi.cug+xml",
        785u16 => "application/vnd.etsi.iptvcommand+xml",
        786u16 => "application/vnd.etsi.iptvdiscovery+xml",
        787u16 => "application/vnd.etsi.iptvprofile+xml",
        788u16 => "application/vnd.etsi.iptvsad-bc+xml",
        789u16 => "application/vnd.etsi.iptvsad-cod+xml",
        790u16 => "application/vnd.etsi.iptvsad-npvr+xml",
        791u16 => "application/vnd.etsi.iptvservice+xml",
        792u16 => "application/vnd.etsi.iptvsync+xml",
        793u16 => "application/vnd.etsi.iptvueprofile+xml",
        794u16 => "application/vnd.etsi.mcid+xml",
        795u16 => "application/vnd.etsi.mheg5",
        796u16 => "application/vnd.etsi.overload-control-policy-dataset+xml",
        797u16 => "application/vnd.etsi.pstn+xml",
        798u16 => "application/vnd.etsi.sci+xml",
        799u16 => "application/vnd.etsi.simservs+xml",
        800u16 => "application/vnd.etsi.timestamp-token",
        801u16 => "application/vnd.etsi.tsl+xml",
        802u16 => "application/vnd.etsi.tsl.der",
        803u16 => "application/vnd.eu.kasparian.car+json",
        804u16 => "application/vnd.eudora.data",
        805u16 => "application/vnd.evolv.ecig.profile",
        806u16 => "application/vnd.evolv.ecig.settings",
        807u16 => "application/vnd.evolv.ecig.theme",
        808u16 => "application/vnd.exstream-empower+zip",
        809u16 => "application/vnd.exstream-package",
        810u16 => "application/vnd.ezpix-album",
        811u16 => "application/vnd.ezpix-package",
        812u16 => "application/vnd.f-secure.mobile",
        813u16 => "application/vnd.familysearch.gedcom+zip",
        814u16 => "application/vnd.fastcopy-disk-image",
        815u16 => "application/vnd.fdsn.mseed",
        816u16 => "application/vnd.fdsn.seed",
        817u16 => "application/vnd.ffsns",
        818u16 => "application/vnd.ficlab.flb+zip",
        819u16 => "application/vnd.filmit.zfc",
        820u16 => "application/vnd.fints",
        821u16 => "application/vnd.firemonkeys.cloudcell",
        822u16 => "application/vnd.fluxtime.clip",
        823u16 => "application/vnd.font-fontforge-sfd",
        824u16 => "application/vnd.framemaker",
        825u16 => "application/vnd.freelog.comic",
        826u16 => "application/vnd.frogans.fnc",
        827u16 => "application/vnd.frogans.ltf",
        828u16 => "application/vnd.fsc.weblaunch",
        829u16 => "application/vnd.fujifilm.fb.docuworks",
        830u16 => "application/vnd.fujifilm.fb.docuworks.binder",
        831u16 => "application/vnd.fujifilm.fb.docuworks.container",
        832u16 => "application/vnd.fujifilm.fb.jfi+xml",
        833u16 => "application/vnd.fujitsu.oasys",
        834u16 => "application/vnd.fujitsu.oasys2",
        835u16 => "application/vnd.fujitsu.oasys3",
        836u16 => "application/vnd.fujitsu.oasysgp",
        837u16 => "application/vnd.fujitsu.oasysprs",
        838u16 => "application/vnd.fujixerox.ART-EX",
        839u16 => "application/vnd.fujixerox.ART4",
        840u16 => "application/vnd.fujixerox.HBPL",
        841u16 => "application/vnd.fujixerox.ddd",
        842u16 => "application/vnd.fujixerox.docuworks",
        843u16 => "application/vnd.fujixerox.docuworks.binder",
        844u16 => "application/vnd.fujixerox.docuworks.container",
        845u16 => "application/vnd.fut-misnet",
        846u16 => "application/vnd.futoin+cbor",
        847u16 => "application/vnd.futoin+json",
        848u16 => "application/vnd.fuzzysheet",
        849u16 => "application/vnd.genomatix.tuxedo",
        850u16 => "application/vnd.genozip",
        851u16 => "application/vnd.gentics.grd+json",
        852u16 => "application/vnd.gentoo.catmetadata+xml",
        853u16 => "application/vnd.gentoo.ebuild",
        854u16 => "application/vnd.gentoo.eclass",
        855u16 => "application/vnd.gentoo.gpkg",
        856u16 => "application/vnd.gentoo.manifest",
        857u16 => "application/vnd.gentoo.pkgmetadata+xml",
        858u16 => "application/vnd.gentoo.xpak",
        859u16 => "application/vnd.geo+json",
        860u16 => "application/vnd.geocube+xml",
        861u16 => "application/vnd.geogebra.file",
        862u16 => "application/vnd.geogebra.slides",
        863u16 => "application/vnd.geogebra.tool",
        864u16 => "application/vnd.geometry-explorer",
        865u16 => "application/vnd.geonext",
        866u16 => "application/vnd.geoplan",
        867u16 => "application/vnd.geospace",
        868u16 => "application/vnd.gerber",
        869u16 => "application/vnd.globalplatform.card-content-mgt",
        870u16 => "application/vnd.globalplatform.card-content-mgt-response",
        871u16 => "application/vnd.gmx",
        872u16 => "application/vnd.gnu.taler.exchange+json",
        873u16 => "application/vnd.gnu.taler.merchant+json",
        874u16 => "application/vnd.google-earth.kml+xml",
        875u16 => "application/vnd.google-earth.kmz",
        876u16 => "application/vnd.gov.sk.e-form+xml",
        877u16 => "application/vnd.gov.sk.e-form+zip",
        878u16 => "application/vnd.gov.sk.xmldatacontainer+xml",
        879u16 => "application/vnd.gpxsee.map+xml",
        880u16 => "application/vnd.grafeq",
        881u16 => "application/vnd.gridmp",
        882u16 => "application/vnd.groove-account",
        883u16 => "application/vnd.groove-help",
        884u16 => "application/vnd.groove-identity-message",
        885u16 => "application/vnd.groove-injector",
        886u16 => "application/vnd.groove-tool-message",
        887u16 => "application/vnd.groove-tool-template",
        888u16 => "application/vnd.groove-vcard",
        889u16 => "application/vnd.hal+json",
        890u16 => "application/vnd.hal+xml",
        891u16 => "application/vnd.hbci",
        892u16 => "application/vnd.hc+json",
        893u16 => "application/vnd.hcl-bireports",
        894u16 => "application/vnd.hdt",
        895u16 => "application/vnd.heroku+json",
        896u16 => "application/vnd.hhe.lesson-player",
        897u16 => "application/vnd.hp-HPGL",
        898u16 => "application/vnd.hp-PCL",
        899u16 => "application/vnd.hp-PCLXL",
        900u16 => "application/vnd.hp-hpid",
        901u16 => "application/vnd.hp-hps",
        902u16 => "application/vnd.hp-jlyt",
        903u16 => "application/vnd.hsl",
        904u16 => "application/vnd.httphone",
        905u16 => "application/vnd.hydrostatix.sof-data",
        906u16 => "application/vnd.hyper+json",
        907u16 => "application/vnd.hyper-item+json",
        908u16 => "application/vnd.hyperdrive+json",
        909u16 => "application/vnd.hzn-3d-crossword",
        910u16 => "application/vnd.ibm.MiniPay",
        911u16 => "application/vnd.ibm.afplinedata",
        912u16 => "application/vnd.ibm.electronic-media",
        913u16 => "application/vnd.ibm.modcap",
        914u16 => "application/vnd.ibm.rights-management",
        915u16 => "application/vnd.ibm.secure-container",
        916u16 => "application/vnd.iccprofile",
        917u16 => "application/vnd.ieee.1905",
        918u16 => "application/vnd.igloader",
        919u16 => "application/vnd.imagemeter.folder+zip",
        920u16 => "application/vnd.imagemeter.image+zip",
        921u16 => "application/vnd.immervision-ivp",
        922u16 => "application/vnd.immervision-ivu",
        923u16 => "application/vnd.ims.imsccv1p1",
        924u16 => "application/vnd.ims.imsccv1p2",
        925u16 => "application/vnd.ims.imsccv1p3",
        926u16 => "application/vnd.ims.lis.v2.result+json",
        927u16 => "application/vnd.ims.lti.v2.toolconsumerprofile+json",
        928u16 => "application/vnd.ims.lti.v2.toolproxy+json",
        929u16 => "application/vnd.ims.lti.v2.toolproxy.id+json",
        930u16 => "application/vnd.ims.lti.v2.toolsettings+json",
        931u16 => "application/vnd.ims.lti.v2.toolsettings.simple+json",
        932u16 => "application/vnd.informedcontrol.rms+xml",
        933u16 => "application/vnd.informix-visionary",
        934u16 => "application/vnd.infotech.project",
        935u16 => "application/vnd.infotech.project+xml",
        936u16 => "application/vnd.innopath.wamp.notification",
        937u16 => "application/vnd.insors.igm",
        938u16 => "application/vnd.intercon.formnet",
        939u16 => "application/vnd.intergeo",
        940u16 => "application/vnd.intertrust.digibox",
        941u16 => "application/vnd.intertrust.nncp",
        942u16 => "application/vnd.intu.qbo",
        943u16 => "application/vnd.intu.qfx",
        944u16 => "application/vnd.ipfs.ipns-record",
        945u16 => "application/vnd.ipld.car",
        946u16 => "application/vnd.ipld.dag-cbor",
        947u16 => "application/vnd.ipld.dag-json",
        948u16 => "application/vnd.ipld.raw",
        949u16 => "application/vnd.iptc.g2.catalogitem+xml",
        950u16 => "application/vnd.iptc.g2.conceptitem+xml",
        951u16 => "application/vnd.iptc.g2.knowledgeitem+xml",
        952u16 => "application/vnd.iptc.g2.newsitem+xml",
        953u16 => "application/vnd.iptc.g2.newsmessage+xml",
        954u16 => "application/vnd.iptc.g2.packageitem+xml",
        955u16 => "application/vnd.iptc.g2.planningitem+xml",
        956u16 => "application/vnd.ipunplugged.rcprofile",
        957u16 => "application/vnd.irepository.package+xml",
        958u16 => "application/vnd.is-xpr",
        959u16 => "application/vnd.isac.fcs",
        960u16 => "application/vnd.iso11783-10+zip",
        961u16 => "application/vnd.jam",
        962u16 => "application/vnd.japannet-directory-service",
        963u16 => "application/vnd.japannet-jpnstore-wakeup",
        964u16 => "application/vnd.japannet-payment-wakeup",
        965u16 => "application/vnd.japannet-registration",
        966u16 => "application/vnd.japannet-registration-wakeup",
        967u16 => "application/vnd.japannet-setstore-wakeup",
        968u16 => "application/vnd.japannet-verification",
        969u16 => "application/vnd.japannet-verification-wakeup",
        970u16 => "application/vnd.jcp.javame.midlet-rms",
        971u16 => "application/vnd.jisp",
        972u16 => "application/vnd.joost.joda-archive",
        973u16 => "application/vnd.jsk.isdn-ngn",
        974u16 => "application/vnd.kahootz",
        975u16 => "application/vnd.kde.karbon",
        976u16 => "application/vnd.kde.kchart",
        977u16 => "application/vnd.kde.kformula",
        978u16 => "application/vnd.kde.kivio",
        979u16 => "application/vnd.kde.kontour",
        980u16 => "application/vnd.kde.kpresenter",
        981u16 => "application/vnd.kde.kspread",
        982u16 => "application/vnd.kde.kword",
        983u16 => "application/vnd.kenameaapp",
        984u16 => "application/vnd.kidspiration",
        985u16 => "application/vnd.koan",
        986u16 => "application/vnd.kodak-descriptor",
        987u16 => "application/vnd.las",
        988u16 => "application/vnd.las.las+json",
        989u16 => "application/vnd.las.las+xml",
        990u16 => "application/vnd.laszip",
        991u16 => "application/vnd.ldev.productlicensing",
        992u16 => "application/vnd.leap+json",
        993u16 => "application/vnd.liberty-request+xml",
        994u16 => "application/vnd.llamagraphics.life-balance.desktop",
        995u16 => "application/vnd.llamagraphics.life-balance.exchange+xml",
        996u16 => "application/vnd.logipipe.circuit+zip",
        997u16 => "application/vnd.loom",
        998u16 => "application/vnd.lotus-1-2-3",
        999u16 => "application/vnd.lotus-approach",
        1000u16 => "application/vnd.lotus-freelance",
        1001u16 => "application/vnd.lotus-notes",
        1002u16 => "application/vnd.lotus-organizer",
        1003u16 => "application/vnd.lotus-screencam",
        1004u16 => "application/vnd.lotus-wordpro",
        1005u16 => "application/vnd.macports.portpkg",
        1006u16 => "application/vnd.mapbox-vector-tile",
        1007u16 => "application/vnd.marlin.drm.actiontoken+xml",
        1008u16 => "application/vnd.marlin.drm.conftoken+xml",
        1009u16 => "application/vnd.marlin.drm.license+xml",
        1010u16 => "application/vnd.marlin.drm.mdcf",
        1011u16 => "application/vnd.mason+json",
        1012u16 => "application/vnd.maxar.archive.3tz+zip",
        1013u16 => "application/vnd.maxmind.maxmind-db",
        1014u16 => "application/vnd.mcd",
        1015u16 => "application/vnd.mdl",
        1016u16 => "application/vnd.mdl-mbsdf",
        1017u16 => "application/vnd.medcalcdata",
        1018u16 => "application/vnd.mediastation.cdkey",
        1019u16 => "application/vnd.medicalholodeck.recordxr",
        1020u16 => "application/vnd.meridian-slingshot",
        1021u16 => "application/vnd.mermaid",
        1022u16 => "application/vnd.mfmp",
        1023u16 => "application/vnd.micro+json",
        1024u16 => "application/vnd.micrografx.flo",
        1025u16 => "application/vnd.micrografx.igx",
        1026u16 => "application/vnd.microsoft.portable-executable",
        1027u16 => "application/vnd.microsoft.windows.thumbnail-cache",
        1028u16 => "application/vnd.miele+json",
        1029u16 => "application/vnd.mif",
        1030u16 => "application/vnd.minisoft-hp3000-save",
        1031u16 => "application/vnd.mitsubishi.misty-guard.trustweb",
        1032u16 => "application/vnd.modl",
        1033u16 => "application/vnd.mophun.application",
        1034u16 => "application/vnd.mophun.certificate",
        1035u16 => "application/vnd.motorola.flexsuite",
        1036u16 => "application/vnd.motorola.flexsuite.adsi",
        1037u16 => "application/vnd.motorola.flexsuite.fis",
        1038u16 => "application/vnd.motorola.flexsuite.gotap",
        1039u16 => "application/vnd.motorola.flexsuite.kmr",
        1040u16 => "application/vnd.motorola.flexsuite.ttc",
        1041u16 => "application/vnd.motorola.flexsuite.wem",
        1042u16 => "application/vnd.motorola.iprm",
        1043u16 => "application/vnd.mozilla.xul+xml",
        1044u16 => "application/vnd.ms-3mfdocument",
        1045u16 => "application/vnd.ms-PrintDeviceCapabilities+xml",
        1046u16 => "application/vnd.ms-PrintSchemaTicket+xml",
        1047u16 => "application/vnd.ms-artgalry",
        1048u16 => "application/vnd.ms-asf",
        1049u16 => "application/vnd.ms-cab-compressed",
        1050u16 => "application/vnd.ms-excel",
        1051u16 => "application/vnd.ms-excel.addin.macroEnabled.12",
        1052u16 => "application/vnd.ms-excel.sheet.binary.macroEnabled.12",
        1053u16 => "application/vnd.ms-excel.sheet.macroEnabled.12",
        1054u16 => "application/vnd.ms-excel.template.macroEnabled.12",
        1055u16 => "application/vnd.ms-fontobject",
        1056u16 => "application/vnd.ms-htmlhelp",
        1057u16 => "application/vnd.ms-ims",
        1058u16 => "application/vnd.ms-lrm",
        1059u16 => "application/vnd.ms-office.activeX+xml",
        1060u16 => "application/vnd.ms-officetheme",
        1061u16 => "application/vnd.ms-playready.initiator+xml",
        1062u16 => "application/vnd.ms-powerpoint",
        1063u16 => "application/vnd.ms-powerpoint.addin.macroEnabled.12",
        1064u16 => "application/vnd.ms-powerpoint.presentation.macroEnabled.12",
        1065u16 => "application/vnd.ms-powerpoint.slide.macroEnabled.12",
        1066u16 => "application/vnd.ms-powerpoint.slideshow.macroEnabled.12",
        1067u16 => "application/vnd.ms-powerpoint.template.macroEnabled.12",
        1068u16 => "application/vnd.ms-project",
        1069u16 => "application/vnd.ms-tnef",
        1070u16 => "application/vnd.ms-windows.devicepairing",
        1071u16 => "application/vnd.ms-windows.nwprinting.oob",
        1072u16 => "application/vnd.ms-windows.printerpairing",
        1073u16 => "application/vnd.ms-windows.wsd.oob",
        1074u16 => "application/vnd.ms-wmdrm.lic-chlg-req",
        1075u16 => "application/vnd.ms-wmdrm.lic-resp",
        1076u16 => "application/vnd.ms-wmdrm.meter-chlg-req",
        1077u16 => "application/vnd.ms-wmdrm.meter-resp",
        1078u16 => "application/vnd.ms-word.document.macroEnabled.12",
        1079u16 => "application/vnd.ms-word.template.macroEnabled.12",
        1080u16 => "application/vnd.ms-works",
        1081u16 => "application/vnd.ms-wpl",
        1082u16 => "application/vnd.ms-xpsdocument",
        1083u16 => "application/vnd.msa-disk-image",
        1084u16 => "application/vnd.mseq",
        1085u16 => "application/vnd.msign",
        1086u16 => "application/vnd.multiad.creator",
        1087u16 => "application/vnd.multiad.creator.cif",
        1088u16 => "application/vnd.music-niff",
        1089u16 => "application/vnd.musician",
        1090u16 => "application/vnd.muvee.style",
        1091u16 => "application/vnd.mynfc",
        1092u16 => "application/vnd.nacamar.ybrid+json",
        1093u16 => "application/vnd.nato.bindingdataobject+cbor",
        1094u16 => "application/vnd.nato.bindingdataobject+json",
        1095u16 => "application/vnd.nato.bindingdataobject+xml",
        1096u16 => "application/vnd.nato.openxmlformats-package.iepd+zip",
        1097u16 => "application/vnd.ncd.control",
        1098u16 => "application/vnd.ncd.reference",
        1099u16 => "application/vnd.nearst.inv+json",
        1100u16 => "application/vnd.nebumind.line",
        1101u16 => "application/vnd.nervana",
        1102u16 => "application/vnd.netfpx",
        1103u16 => "application/vnd.neurolanguage.nlu",
        1104u16 => "application/vnd.nimn",
        1105u16 => "application/vnd.nintendo.nitro.rom",
        1106u16 => "application/vnd.nintendo.snes.rom",
        1107u16 => "application/vnd.nitf",
        1108u16 => "application/vnd.noblenet-directory",
        1109u16 => "application/vnd.noblenet-sealer",
        1110u16 => "application/vnd.noblenet-web",
        1111u16 => "application/vnd.nokia.catalogs",
        1112u16 => "application/vnd.nokia.conml+wbxml",
        1113u16 => "application/vnd.nokia.conml+xml",
        1114u16 => "application/vnd.nokia.iSDS-radio-presets",
        1115u16 => "application/vnd.nokia.iptv.config+xml",
        1116u16 => "application/vnd.nokia.landmark+wbxml",
        1117u16 => "application/vnd.nokia.landmark+xml",
        1118u16 => "application/vnd.nokia.landmarkcollection+xml",
        1119u16 => "application/vnd.nokia.n-gage.ac+xml",
        1120u16 => "application/vnd.nokia.n-gage.data",
        1121u16 => "application/vnd.nokia.n-gage.symbian.install",
        1122u16 => "application/vnd.nokia.ncd",
        1123u16 => "application/vnd.nokia.pcd+wbxml",
        1124u16 => "application/vnd.nokia.pcd+xml",
        1125u16 => "application/vnd.nokia.radio-preset",
        1126u16 => "application/vnd.nokia.radio-presets",
        1127u16 => "application/vnd.novadigm.EDM",
        1128u16 => "application/vnd.novadigm.EDX",
        1129u16 => "application/vnd.novadigm.EXT",
        1130u16 => "application/vnd.ntt-local.content-share",
        1131u16 => "application/vnd.ntt-local.file-transfer",
        1132u16 => "application/vnd.ntt-local.ogw_remote-access",
        1133u16 => "application/vnd.ntt-local.sip-ta_remote",
        1134u16 => "application/vnd.ntt-local.sip-ta_tcp_stream",
        1135u16 => "application/vnd.oai.workflows",
        1136u16 => "application/vnd.oai.workflows+json",
        1137u16 => "application/vnd.oai.workflows+yaml",
        1138u16 => "application/vnd.oasis.opendocument.base",
        1139u16 => "application/vnd.oasis.opendocument.chart",
        1140u16 => "application/vnd.oasis.opendocument.chart-template",
        1141u16 => "application/vnd.oasis.opendocument.database",
        1142u16 => "application/vnd.oasis.opendocument.formula",
        1143u16 => "application/vnd.oasis.opendocument.formula-template",
        1144u16 => "application/vnd.oasis.opendocument.graphics",
        1145u16 => "application/vnd.oasis.opendocument.graphics-template",
        1146u16 => "application/vnd.oasis.opendocument.image",
        1147u16 => "application/vnd.oasis.opendocument.image-template",
        1148u16 => "application/vnd.oasis.opendocument.presentation",
        1149u16 => "application/vnd.oasis.opendocument.presentation-template",
        1150u16 => "application/vnd.oasis.opendocument.spreadsheet",
        1151u16 => "application/vnd.oasis.opendocument.spreadsheet-template",
        1152u16 => "application/vnd.oasis.opendocument.text",
        1153u16 => "application/vnd.oasis.opendocument.text-master",
        1154u16 => "application/vnd.oasis.opendocument.text-master-template",
        1155u16 => "application/vnd.oasis.opendocument.text-template",
        1156u16 => "application/vnd.oasis.opendocument.text-web",
        1157u16 => "application/vnd.obn",
        1158u16 => "application/vnd.ocf+cbor",
        1159u16 => "application/vnd.oci.image.manifest.v1+json",
        1160u16 => "application/vnd.oftn.l10n+json",
        1161u16 => "application/vnd.oipf.contentaccessdownload+xml",
        1162u16 => "application/vnd.oipf.contentaccessstreaming+xml",
        1163u16 => "application/vnd.oipf.cspg-hexbinary",
        1164u16 => "application/vnd.oipf.dae.svg+xml",
        1165u16 => "application/vnd.oipf.dae.xhtml+xml",
        1166u16 => "application/vnd.oipf.mippvcontrolmessage+xml",
        1167u16 => "application/vnd.oipf.pae.gem",
        1168u16 => "application/vnd.oipf.spdiscovery+xml",
        1169u16 => "application/vnd.oipf.spdlist+xml",
        1170u16 => "application/vnd.oipf.ueprofile+xml",
        1171u16 => "application/vnd.oipf.userprofile+xml",
        1172u16 => "application/vnd.olpc-sugar",
        1173u16 => "application/vnd.oma-scws-config",
        1174u16 => "application/vnd.oma-scws-http-request",
        1175u16 => "application/vnd.oma-scws-http-response",
        1176u16 => "application/vnd.oma.bcast.associated-procedure-parameter+xml",
        1177u16 => "application/vnd.oma.bcast.drm-trigger+xml",
        1178u16 => "application/vnd.oma.bcast.imd+xml",
        1179u16 => "application/vnd.oma.bcast.ltkm",
        1180u16 => "application/vnd.oma.bcast.notification+xml",
        1181u16 => "application/vnd.oma.bcast.provisioningtrigger",
        1182u16 => "application/vnd.oma.bcast.sgboot",
        1183u16 => "application/vnd.oma.bcast.sgdd+xml",
        1184u16 => "application/vnd.oma.bcast.sgdu",
        1185u16 => "application/vnd.oma.bcast.simple-symbol-container",
        1186u16 => "application/vnd.oma.bcast.smartcard-trigger+xml",
        1187u16 => "application/vnd.oma.bcast.sprov+xml",
        1188u16 => "application/vnd.oma.bcast.stkm",
        1189u16 => "application/vnd.oma.cab-address-book+xml",
        1190u16 => "application/vnd.oma.cab-feature-handler+xml",
        1191u16 => "application/vnd.oma.cab-pcc+xml",
        1192u16 => "application/vnd.oma.cab-subs-invite+xml",
        1193u16 => "application/vnd.oma.cab-user-prefs+xml",
        1194u16 => "application/vnd.oma.dcd",
        1195u16 => "application/vnd.oma.dcdc",
        1196u16 => "application/vnd.oma.dd2+xml",
        1197u16 => "application/vnd.oma.drm.risd+xml",
        1198u16 => "application/vnd.oma.group-usage-list+xml",
        1199u16 => "application/vnd.oma.lwm2m+cbor",
        1200u16 => "application/vnd.oma.lwm2m+json",
        1201u16 => "application/vnd.oma.lwm2m+tlv",
        1202u16 => "application/vnd.oma.pal+xml",
        1203u16 => "application/vnd.oma.poc.detailed-progress-report+xml",
        1204u16 => "application/vnd.oma.poc.final-report+xml",
        1205u16 => "application/vnd.oma.poc.groups+xml",
        1206u16 => "application/vnd.oma.poc.invocation-descriptor+xml",
        1207u16 => "application/vnd.oma.poc.optimized-progress-report+xml",
        1208u16 => "application/vnd.oma.push",
        1209u16 => "application/vnd.oma.scidm.messages+xml",
        1210u16 => "application/vnd.oma.xcap-directory+xml",
        1211u16 => "application/vnd.omads-email+xml",
        1212u16 => "application/vnd.omads-file+xml",
        1213u16 => "application/vnd.omads-folder+xml",
        1214u16 => "application/vnd.omaloc-supl-init",
        1215u16 => "application/vnd.onepager",
        1216u16 => "application/vnd.onepagertamp",
        1217u16 => "application/vnd.onepagertamx",
        1218u16 => "application/vnd.onepagertat",
        1219u16 => "application/vnd.onepagertatp",
        1220u16 => "application/vnd.onepagertatx",
        1221u16 => "application/vnd.onvif.metadata",
        1222u16 => "application/vnd.openblox.game+xml",
        1223u16 => "application/vnd.openblox.game-binary",
        1224u16 => "application/vnd.openeye.oeb",
        1225u16 => "application/vnd.openstreetmap.data+xml",
        1226u16 => "application/vnd.opentimestamps.ots",
        1227u16 => "application/vnd.openxmlformats-officedocument.custom-properties+xml",
        1228u16 => "application/vnd.openxmlformats-officedocument.customXmlProperties+xml",
        1229u16 => "application/vnd.openxmlformats-officedocument.drawing+xml",
        1230u16 => "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
        1231u16 => "application/vnd.openxmlformats-officedocument.drawingml.chartshapes+xml",
        1232u16 => "application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml",
        1233u16 => "application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml",
        1234u16 => "application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml",
        1235u16 => "application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml",
        1236u16 => "application/vnd.openxmlformats-officedocument.extended-properties+xml",
        1237u16 => "application/vnd.openxmlformats-officedocument.presentationml.commentAuthors+xml",
        1238u16 => "application/vnd.openxmlformats-officedocument.presentationml.comments+xml",
        1239u16 => "application/vnd.openxmlformats-officedocument.presentationml.handoutMaster+xml",
        1240u16 => "application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml",
        1241u16 => "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml",
        1242u16 => "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml",
        1243u16 => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        1244u16 => "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
        1245u16 => "application/vnd.openxmlformats-officedocument.presentationml.slide",
        1246u16 => "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
        1247u16 => "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml",
        1248u16 => "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml",
        1249u16 => "application/vnd.openxmlformats-officedocument.presentationml.slideUpdateInfo+xml",
        1250u16 => "application/vnd.openxmlformats-officedocument.presentationml.slideshow",
        1251u16 => "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml",
        1252u16 => "application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml",
        1253u16 => "application/vnd.openxmlformats-officedocument.presentationml.tags+xml",
        1254u16 => "application/vnd.openxmlformats-officedocument.presentationml.template",
        1255u16 => "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml",
        1256u16 => "application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml",
        1257u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml",
        1258u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml",
        1259u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml",
        1260u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml",
        1261u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.dialogsheet+xml",
        1262u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml",
        1263u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml",
        1264u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml",
        1265u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml",
        1266u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.queryTable+xml",
        1267u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionHeaders+xml",
        1268u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionLog+xml",
        1269u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml",
        1270u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        1271u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        1272u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml",
        1273u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml",
        1274u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml",
        1275u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.tableSingleCells+xml",
        1276u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.template",
        1277u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml",
        1278u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.userNames+xml",
        1279u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.volatileDependencies+xml",
        1280u16 => "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
        1281u16 => "application/vnd.openxmlformats-officedocument.theme+xml",
        1282u16 => "application/vnd.openxmlformats-officedocument.themeOverride+xml",
        1283u16 => "application/vnd.openxmlformats-officedocument.vmlDrawing",
        1284u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        1285u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        1286u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.document.glossary+xml",
        1287u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        1288u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml",
        1289u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml",
        1290u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
        1291u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
        1292u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
        1293u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml",
        1294u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
        1295u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.template",
        1296u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml",
        1297u16 => "application/vnd.openxmlformats-officedocument.wordprocessingml.webSettings+xml",
        1298u16 => "application/vnd.openxmlformats-package.core-properties+xml",
        1299u16 => "application/vnd.openxmlformats-package.digital-signature-xmlsignature+xml",
        1300u16 => "application/vnd.openxmlformats-package.relationships+xml",
        1301u16 => "application/vnd.oracle.resource+json",
        1302u16 => "application/vnd.orange.indata",
        1303u16 => "application/vnd.osa.netdeploy",
        1304u16 => "application/vnd.osgeo.mapguide.package",
        1305u16 => "application/vnd.osgi.bundle",
        1306u16 => "application/vnd.osgi.dp",
        1307u16 => "application/vnd.osgi.subsystem",
        1308u16 => "application/vnd.otps.ct-kip+xml",
        1309u16 => "application/vnd.oxli.countgraph",
        1310u16 => "application/vnd.pagerduty+json",
        1311u16 => "application/vnd.palm",
        1312u16 => "application/vnd.panoply",
        1313u16 => "application/vnd.paos.xml",
        1314u16 => "application/vnd.patentdive",
        1315u16 => "application/vnd.patientecommsdoc",
        1316u16 => "application/vnd.pawaafile",
        1317u16 => "application/vnd.pcos",
        1318u16 => "application/vnd.pg.format",
        1319u16 => "application/vnd.pg.osasli",
        1320u16 => "application/vnd.piaccess.application-licence",
        1321u16 => "application/vnd.picsel",
        1322u16 => "application/vnd.pmi.widget",
        1323u16 => "application/vnd.poc.group-advertisement+xml",
        1324u16 => "application/vnd.pocketlearn",
        1325u16 => "application/vnd.powerbuilder6",
        1326u16 => "application/vnd.powerbuilder6-s",
        1327u16 => "application/vnd.powerbuilder7",
        1328u16 => "application/vnd.powerbuilder7-s",
        1329u16 => "application/vnd.powerbuilder75",
        1330u16 => "application/vnd.powerbuilder75-s",
        1331u16 => "application/vnd.preminet",
        1332u16 => "application/vnd.previewsystems.box",
        1333u16 => "application/vnd.proteus.magazine",
        1334u16 => "application/vnd.psfs",
        1335u16 => "application/vnd.pt.mundusmundi",
        1336u16 => "application/vnd.publishare-delta-tree",
        1337u16 => "application/vnd.pvi.ptid1",
        1338u16 => "application/vnd.pwg-multiplexed",
        1339u16 => "application/vnd.pwg-xhtml-print+xml",
        1340u16 => "application/vnd.qualcomm.brew-app-res",
        1341u16 => "application/vnd.quarantainenet",
        1342u16 => "application/vnd.quobject-quoxdocument",
        1343u16 => "application/vnd.radisys.moml+xml",
        1344u16 => "application/vnd.radisys.msml+xml",
        1345u16 => "application/vnd.radisys.msml-audit+xml",
        1346u16 => "application/vnd.radisys.msml-audit-conf+xml",
        1347u16 => "application/vnd.radisys.msml-audit-conn+xml",
        1348u16 => "application/vnd.radisys.msml-audit-dialog+xml",
        1349u16 => "application/vnd.radisys.msml-audit-stream+xml",
        1350u16 => "application/vnd.radisys.msml-conf+xml",
        1351u16 => "application/vnd.radisys.msml-dialog+xml",
        1352u16 => "application/vnd.radisys.msml-dialog-base+xml",
        1353u16 => "application/vnd.radisys.msml-dialog-fax-detect+xml",
        1354u16 => "application/vnd.radisys.msml-dialog-fax-sendrecv+xml",
        1355u16 => "application/vnd.radisys.msml-dialog-group+xml",
        1356u16 => "application/vnd.radisys.msml-dialog-speech+xml",
        1357u16 => "application/vnd.radisys.msml-dialog-transform+xml",
        1358u16 => "application/vnd.rainstor.data",
        1359u16 => "application/vnd.rapid",
        1360u16 => "application/vnd.rar",
        1361u16 => "application/vnd.realvnc.bed",
        1362u16 => "application/vnd.recordare.musicxml",
        1363u16 => "application/vnd.recordare.musicxml+xml",
        1364u16 => "application/vnd.relpipe",
        1365u16 => "application/vnd.resilient.logic",
        1366u16 => "application/vnd.restful+json",
        1367u16 => "application/vnd.rig.cryptonote",
        1368u16 => "application/vnd.route66.link66+xml",
        1369u16 => "application/vnd.rs-274x",
        1370u16 => "application/vnd.ruckus.download",
        1371u16 => "application/vnd.s3sms",
        1372u16 => "application/vnd.sailingtracker.track",
        1373u16 => "application/vnd.sar",
        1374u16 => "application/vnd.sbm.cid",
        1375u16 => "application/vnd.sbm.mid2",
        1376u16 => "application/vnd.scribus",
        1377u16 => "application/vnd.sealed.3df",
        1378u16 => "application/vnd.sealed.csf",
        1379u16 => "application/vnd.sealed.doc",
        1380u16 => "application/vnd.sealed.eml",
        1381u16 => "application/vnd.sealed.mht",
        1382u16 => "application/vnd.sealed.net",
        1383u16 => "application/vnd.sealed.ppt",
        1384u16 => "application/vnd.sealed.tiff",
        1385u16 => "application/vnd.sealed.xls",
        1386u16 => "application/vnd.sealedmedia.softseal.html",
        1387u16 => "application/vnd.sealedmedia.softseal.pdf",
        1388u16 => "application/vnd.seemail",
        1389u16 => "application/vnd.seis+json",
        1390u16 => "application/vnd.sema",
        1391u16 => "application/vnd.semd",
        1392u16 => "application/vnd.semf",
        1393u16 => "application/vnd.shade-save-file",
        1394u16 => "application/vnd.shana.informed.formdata",
        1395u16 => "application/vnd.shana.informed.formtemplate",
        1396u16 => "application/vnd.shana.informed.interchange",
        1397u16 => "application/vnd.shana.informed.package",
        1398u16 => "application/vnd.shootproof+json",
        1399u16 => "application/vnd.shopkick+json",
        1400u16 => "application/vnd.shp",
        1401u16 => "application/vnd.shx",
        1402u16 => "application/vnd.sigrok.session",
        1403u16 => "application/vnd.siren+json",
        1404u16 => "application/vnd.smaf",
        1405u16 => "application/vnd.smart.notebook",
        1406u16 => "application/vnd.smart.teacher",
        1407u16 => "application/vnd.smintio.portals.archive",
        1408u16 => "application/vnd.snesdev-page-table",
        1409u16 => "application/vnd.software602.filler.form+xml",
        1410u16 => "application/vnd.software602.filler.form-xml-zip",
        1411u16 => "application/vnd.solent.sdkm+xml",
        1412u16 => "application/vnd.spotfire.dxp",
        1413u16 => "application/vnd.spotfire.sfs",
        1414u16 => "application/vnd.sqlite3",
        1415u16 => "application/vnd.sss-cod",
        1416u16 => "application/vnd.sss-dtf",
        1417u16 => "application/vnd.sss-ntf",
        1418u16 => "application/vnd.stepmania.package",
        1419u16 => "application/vnd.stepmania.stepchart",
        1420u16 => "application/vnd.street-stream",
        1421u16 => "application/vnd.sun.wadl+xml",
        1422u16 => "application/vnd.sus-calendar",
        1423u16 => "application/vnd.svd",
        1424u16 => "application/vnd.swiftview-ics",
        1425u16 => "application/vnd.sybyl.mol2",
        1426u16 => "application/vnd.sycle+xml",
        1427u16 => "application/vnd.syft+json",
        1428u16 => "application/vnd.syncml+xml",
        1429u16 => "application/vnd.syncml.dm+wbxml",
        1430u16 => "application/vnd.syncml.dm+xml",
        1431u16 => "application/vnd.syncml.dm.notification",
        1432u16 => "application/vnd.syncml.dmddf+wbxml",
        1433u16 => "application/vnd.syncml.dmddf+xml",
        1434u16 => "application/vnd.syncml.dmtnds+wbxml",
        1435u16 => "application/vnd.syncml.dmtnds+xml",
        1436u16 => "application/vnd.syncml.ds.notification",
        1437u16 => "application/vnd.tableschema+json",
        1438u16 => "application/vnd.tao.intent-module-archive",
        1439u16 => "application/vnd.tcpdump.pcap",
        1440u16 => "application/vnd.think-cell.ppttc+json",
        1441u16 => "application/vnd.tmd.mediaflex.api+xml",
        1442u16 => "application/vnd.tml",
        1443u16 => "application/vnd.tmobile-livetv",
        1444u16 => "application/vnd.tri.onesource",
        1445u16 => "application/vnd.trid.tpt",
        1446u16 => "application/vnd.triscape.mxs",
        1447u16 => "application/vnd.trueapp",
        1448u16 => "application/vnd.truedoc",
        1449u16 => "application/vnd.ubisoft.webplayer",
        1450u16 => "application/vnd.ufdl",
        1451u16 => "application/vnd.uiq.theme",
        1452u16 => "application/vnd.umajin",
        1453u16 => "application/vnd.unity",
        1454u16 => "application/vnd.uoml+xml",
        1455u16 => "application/vnd.uplanet.alert",
        1456u16 => "application/vnd.uplanet.alert-wbxml",
        1457u16 => "application/vnd.uplanet.bearer-choice",
        1458u16 => "application/vnd.uplanet.bearer-choice-wbxml",
        1459u16 => "application/vnd.uplanet.cacheop",
        1460u16 => "application/vnd.uplanet.cacheop-wbxml",
        1461u16 => "application/vnd.uplanet.channel",
        1462u16 => "application/vnd.uplanet.channel-wbxml",
        1463u16 => "application/vnd.uplanet.list",
        1464u16 => "application/vnd.uplanet.list-wbxml",
        1465u16 => "application/vnd.uplanet.listcmd",
        1466u16 => "application/vnd.uplanet.listcmd-wbxml",
        1467u16 => "application/vnd.uplanet.signal",
        1468u16 => "application/vnd.uri-map",
        1469u16 => "application/vnd.valve.source.material",
        1470u16 => "application/vnd.vcx",
        1471u16 => "application/vnd.vd-study",
        1472u16 => "application/vnd.vectorworks",
        1473u16 => "application/vnd.vel+json",
        1474u16 => "application/vnd.verimatrix.vcas",
        1475u16 => "application/vnd.veritone.aion+json",
        1476u16 => "application/vnd.veryant.thin",
        1477u16 => "application/vnd.ves.encrypted",
        1478u16 => "application/vnd.vidsoft.vidconference",
        1479u16 => "application/vnd.visio",
        1480u16 => "application/vnd.visionary",
        1481u16 => "application/vnd.vividence.scriptfile",
        1482u16 => "application/vnd.vsf",
        1483u16 => "application/vnd.wap.sic",
        1484u16 => "application/vnd.wap.slc",
        1485u16 => "application/vnd.wap.wbxml",
        1486u16 => "application/vnd.wap.wmlc",
        1487u16 => "application/vnd.wap.wmlscriptc",
        1488u16 => "application/vnd.wasmflow.wafl",
        1489u16 => "application/vnd.webturbo",
        1490u16 => "application/vnd.wfa.dpp",
        1491u16 => "application/vnd.wfa.p2p",
        1492u16 => "application/vnd.wfa.wsc",
        1493u16 => "application/vnd.windows.devicepairing",
        1494u16 => "application/vnd.wmc",
        1495u16 => "application/vnd.wmf.bootstrap",
        1496u16 => "application/vnd.wolfram.mathematica",
        1497u16 => "application/vnd.wolfram.mathematica.package",
        1498u16 => "application/vnd.wolfram.player",
        1499u16 => "application/vnd.wordlift",
        1500u16 => "application/vnd.wordperfect",
        1501u16 => "application/vnd.wqd",
        1502u16 => "application/vnd.wrq-hp3000-labelled",
        1503u16 => "application/vnd.wt.stf",
        1504u16 => "application/vnd.wv.csp+wbxml",
        1505u16 => "application/vnd.wv.csp+xml",
        1506u16 => "application/vnd.wv.ssp+xml",
        1507u16 => "application/vnd.xacml+json",
        1508u16 => "application/vnd.xara",
        1509u16 => "application/vnd.xecrets-encrypted",
        1510u16 => "application/vnd.xfdl",
        1511u16 => "application/vnd.xfdl.webform",
        1512u16 => "application/vnd.xmi+xml",
        1513u16 => "application/vnd.xmpie.cpkg",
        1514u16 => "application/vnd.xmpie.dpkg",
        1515u16 => "application/vnd.xmpie.plan",
        1516u16 => "application/vnd.xmpie.ppkg",
        1517u16 => "application/vnd.xmpie.xlim",
        1518u16 => "application/vnd.yamaha.hv-dic",
        1519u16 => "application/vnd.yamaha.hv-script",
        1520u16 => "application/vnd.yamaha.hv-voice",
        1521u16 => "application/vnd.yamaha.openscoreformat",
        1522u16 => "application/vnd.yamaha.openscoreformat.osfpvg+xml",
        1523u16 => "application/vnd.yamaha.remote-setup",
        1524u16 => "application/vnd.yamaha.smaf-audio",
        1525u16 => "application/vnd.yamaha.smaf-phrase",
        1526u16 => "application/vnd.yamaha.through-ngn",
        1527u16 => "application/vnd.yamaha.tunnel-udpencap",
        1528u16 => "application/vnd.yaoweme",
        1529u16 => "application/vnd.yellowriver-custom-menu",
        1530u16 => "application/vnd.youtube.yt",
        1531u16 => "application/vnd.zul",
        1532u16 => "application/vnd.zzazz.deck+xml",
        1533u16 => "application/voicexml+xml",
        1534u16 => "application/voucher-cms+json",
        1535u16 => "application/vq-rtcpxr",
        1536u16 => "application/wasm",
        1537u16 => "application/watcherinfo+xml",
        1538u16 => "application/webpush-options+json",
        1539u16 => "application/whoispp-query",
        1540u16 => "application/whoispp-response",
        1541u16 => "application/widget",
        1542u16 => "application/wita",
        1543u16 => "application/wordperfect5.1",
        1544u16 => "application/wsdl+xml",
        1545u16 => "application/wspolicy+xml",
        1546u16 => "application/x-pki-message",
        1547u16 => "application/x-www-form-urlencoded",
        1548u16 => "application/x-x509-ca-cert",
        1549u16 => "application/x-x509-ca-ra-cert",
        1550u16 => "application/x-x509-next-ca-cert",
        1551u16 => "application/x400-bp",
        1552u16 => "application/xacml+xml",
        1553u16 => "application/xcap-att+xml",
        1554u16 => "application/xcap-caps+xml",
        1555u16 => "application/xcap-diff+xml",
        1556u16 => "application/xcap-el+xml",
        1557u16 => "application/xcap-error+xml",
        1558u16 => "application/xcap-ns+xml",
        1559u16 => "application/xcon-conference-info+xml",
        1560u16 => "application/xcon-conference-info-diff+xml",
        1561u16 => "application/xenc+xml",
        1562u16 => "application/xfdf",
        1563u16 => "application/xhtml+xml",
        1564u16 => "application/xliff+xml",
        1565u16 => "application/xml",
        1566u16 => "application/xml-dtd",
        1567u16 => "application/xml-external-parsed-entity",
        1568u16 => "application/xml-patch+xml",
        1569u16 => "application/xmpp+xml",
        1570u16 => "application/xop+xml",
        1571u16 => "application/xslt+xml",
        1572u16 => "application/xv+xml",
        1573u16 => "application/yaml",
        1574u16 => "application/yang",
        1575u16 => "application/yang-data+cbor",
        1576u16 => "application/yang-data+json",
        1577u16 => "application/yang-data+xml",
        1578u16 => "application/yang-patch+json",
        1579u16 => "application/yang-patch+xml",
        1580u16 => "application/yang-sid+json",
        1581u16 => "application/yin+xml",
        1582u16 => "application/zip",
        1583u16 => "application/zlib",
        1584u16 => "application/zstd",
        1585u16 => "audio/1d-interleaved-parityfec",
        1586u16 => "audio/32kadpcm",
        1587u16 => "audio/3gpp",
        1588u16 => "audio/3gpp2",
        1589u16 => "audio/AMR",
        1590u16 => "audio/AMR-WB",
        1591u16 => "audio/ATRAC-ADVANCED-LOSSLESS",
        1592u16 => "audio/ATRAC-X",
        1593u16 => "audio/ATRAC3",
        1594u16 => "audio/BV16",
        1595u16 => "audio/BV32",
        1596u16 => "audio/CN",
        1597u16 => "audio/DAT12",
        1598u16 => "audio/DV",
        1599u16 => "audio/DVI4",
        1600u16 => "audio/EVRC",
        1601u16 => "audio/EVRC-QCP",
        1602u16 => "audio/EVRC0",
        1603u16 => "audio/EVRC1",
        1604u16 => "audio/EVRCB",
        1605u16 => "audio/EVRCB0",
        1606u16 => "audio/EVRCB1",
        1607u16 => "audio/EVRCNW",
        1608u16 => "audio/EVRCNW0",
        1609u16 => "audio/EVRCNW1",
        1610u16 => "audio/EVRCWB",
        1611u16 => "audio/EVRCWB0",
        1612u16 => "audio/EVRCWB1",
        1613u16 => "audio/EVS",
        1614u16 => "audio/G711-0",
        1615u16 => "audio/G719",
        1616u16 => "audio/G722",
        1617u16 => "audio/G7221",
        1618u16 => "audio/G723",
        1619u16 => "audio/G726-16",
        1620u16 => "audio/G726-24",
        1621u16 => "audio/G726-32",
        1622u16 => "audio/G726-40",
        1623u16 => "audio/G728",
        1624u16 => "audio/G729",
        1625u16 => "audio/G7291",
        1626u16 => "audio/G729D",
        1627u16 => "audio/G729E",
        1628u16 => "audio/GSM",
        1629u16 => "audio/GSM-EFR",
        1630u16 => "audio/GSM-HR-08",
        1631u16 => "audio/L16",
        1632u16 => "audio/L20",
        1633u16 => "audio/L24",
        1634u16 => "audio/L8",
        1635u16 => "audio/LPC",
        1636u16 => "audio/MELP",
        1637u16 => "audio/MELP1200",
        1638u16 => "audio/MELP2400",
        1639u16 => "audio/MELP600",
        1640u16 => "audio/MP4A-LATM",
        1641u16 => "audio/MPA",
        1642u16 => "audio/PCMA",
        1643u16 => "audio/PCMA-WB",
        1644u16 => "audio/PCMU",
        1645u16 => "audio/PCMU-WB",
        1646u16 => "audio/QCELP",
        1647u16 => "audio/RED",
        1648u16 => "audio/SMV",
        1649u16 => "audio/SMV-QCP",
        1650u16 => "audio/SMV0",
        1651u16 => "audio/TETRA_ACELP",
        1652u16 => "audio/TETRA_ACELP_BB",
        1653u16 => "audio/TSVCIS",
        1654u16 => "audio/UEMCLIP",
        1655u16 => "audio/VDVI",
        1656u16 => "audio/VMR-WB",
        1657u16 => "audio/aac",
        1658u16 => "audio/ac3",
        1659u16 => "audio/amr-wb+",
        1660u16 => "audio/aptx",
        1661u16 => "audio/asc",
        1662u16 => "audio/basic",
        1663u16 => "audio/clearmode",
        1664u16 => "audio/dls",
        1665u16 => "audio/dsr-es201108",
        1666u16 => "audio/dsr-es202050",
        1667u16 => "audio/dsr-es202211",
        1668u16 => "audio/dsr-es202212",
        1669u16 => "audio/eac3",
        1670u16 => "audio/encaprtp",
        1671u16 => "audio/example",
        1672u16 => "audio/flexfec",
        1673u16 => "audio/fwdred",
        1674u16 => "audio/iLBC",
        1675u16 => "audio/ip-mr_v2.5",
        1676u16 => "audio/matroska",
        1677u16 => "audio/mhas",
        1678u16 => "audio/mobile-xmf",
        1679u16 => "audio/mp4",
        1680u16 => "audio/mpa-robust",
        1681u16 => "audio/mpeg",
        1682u16 => "audio/mpeg4-generic",
        1683u16 => "audio/ogg",
        1684u16 => "audio/opus",
        1685u16 => "audio/parityfec",
        1686u16 => "audio/prs.sid",
        1687u16 => "audio/raptorfec",
        1688u16 => "audio/rtp-enc-aescm128",
        1689u16 => "audio/rtp-midi",
        1690u16 => "audio/rtploopback",
        1691u16 => "audio/rtx",
        1692u16 => "audio/scip",
        1693u16 => "audio/sofa",
        1694u16 => "audio/sp-midi",
        1695u16 => "audio/speex",
        1696u16 => "audio/t140c",
        1697u16 => "audio/t38",
        1698u16 => "audio/telephone-event",
        1699u16 => "audio/tone",
        1700u16 => "audio/ulpfec",
        1701u16 => "audio/usac",
        1702u16 => "audio/vnd.3gpp.iufp",
        1703u16 => "audio/vnd.4SB",
        1704u16 => "audio/vnd.CELP",
        1705u16 => "audio/vnd.audiokoz",
        1706u16 => "audio/vnd.cisco.nse",
        1707u16 => "audio/vnd.cmles.radio-events",
        1708u16 => "audio/vnd.cns.anp1",
        1709u16 => "audio/vnd.cns.inf1",
        1710u16 => "audio/vnd.dece.audio",
        1711u16 => "audio/vnd.digital-winds",
        1712u16 => "audio/vnd.dlna.adts",
        1713u16 => "audio/vnd.dolby.heaac.1",
        1714u16 => "audio/vnd.dolby.heaac.2",
        1715u16 => "audio/vnd.dolby.mlp",
        1716u16 => "audio/vnd.dolby.mps",
        1717u16 => "audio/vnd.dolby.pl2",
        1718u16 => "audio/vnd.dolby.pl2x",
        1719u16 => "audio/vnd.dolby.pl2z",
        1720u16 => "audio/vnd.dolby.pulse.1",
        1721u16 => "audio/vnd.dra",
        1722u16 => "audio/vnd.dts",
        1723u16 => "audio/vnd.dts.hd",
        1724u16 => "audio/vnd.dts.uhd",
        1725u16 => "audio/vnd.dvb.file",
        1726u16 => "audio/vnd.everad.plj",
        1727u16 => "audio/vnd.hns.audio",
        1728u16 => "audio/vnd.lucent.voice",
        1729u16 => "audio/vnd.ms-playready.media.pya",
        1730u16 => "audio/vnd.nokia.mobile-xmf",
        1731u16 => "audio/vnd.nortel.vbk",
        1732u16 => "audio/vnd.nuera.ecelp4800",
        1733u16 => "audio/vnd.nuera.ecelp7470",
        1734u16 => "audio/vnd.nuera.ecelp9600",
        1735u16 => "audio/vnd.octel.sbc",
        1736u16 => "audio/vnd.presonus.multitrack",
        1737u16 => "audio/vnd.qcelp",
        1738u16 => "audio/vnd.rhetorex.32kadpcm",
        1739u16 => "audio/vnd.rip",
        1740u16 => "audio/vnd.sealedmedia.softseal.mpeg",
        1741u16 => "audio/vnd.vmx.cvsd",
        1742u16 => "audio/vorbis",
        1743u16 => "audio/vorbis-config",
        1744u16 => "font/collection",
        1745u16 => "font/otf",
        1746u16 => "font/sfnt",
        1747u16 => "font/ttf",
        1748u16 => "font/woff",
        1749u16 => "font/woff2",
        1750u16 => "image/aces",
        1751u16 => "image/apng",
        1752u16 => "image/avci",
        1753u16 => "image/avcs",
        1754u16 => "image/avif",
        1755u16 => "image/bmp",
        1756u16 => "image/cgm",
        1757u16 => "image/dicom-rle",
        1758u16 => "image/dpx",
        1759u16 => "image/emf",
        1760u16 => "image/example",
        1761u16 => "image/fits",
        1762u16 => "image/g3fax",
        1763u16 => "image/gif",
        1764u16 => "image/heic",
        1765u16 => "image/heic-sequence",
        1766u16 => "image/heif",
        1767u16 => "image/heif-sequence",
        1768u16 => "image/hej2k",
        1769u16 => "image/hsj2",
        1770u16 => "image/ief",
        1771u16 => "image/j2c",
        1772u16 => "image/jls",
        1773u16 => "image/jp2",
        1774u16 => "image/jpeg",
        1775u16 => "image/jph",
        1776u16 => "image/jphc",
        1777u16 => "image/jpm",
        1778u16 => "image/jpx",
        1779u16 => "image/jxr",
        1780u16 => "image/jxrA",
        1781u16 => "image/jxrS",
        1782u16 => "image/jxs",
        1783u16 => "image/jxsc",
        1784u16 => "image/jxsi",
        1785u16 => "image/jxss",
        1786u16 => "image/ktx",
        1787u16 => "image/ktx2",
        1788u16 => "image/naplps",
        1789u16 => "image/png",
        1790u16 => "image/prs.btif",
        1791u16 => "image/prs.pti",
        1792u16 => "image/pwg-raster",
        1793u16 => "image/svg+xml",
        1794u16 => "image/t38",
        1795u16 => "image/tiff",
        1796u16 => "image/tiff-fx",
        1797u16 => "image/vnd.adobe.photoshop",
        1798u16 => "image/vnd.airzip.accelerator.azv",
        1799u16 => "image/vnd.cns.inf2",
        1800u16 => "image/vnd.dece.graphic",
        1801u16 => "image/vnd.djvu",
        1802u16 => "image/vnd.dvb.subtitle",
        1803u16 => "image/vnd.dwg",
        1804u16 => "image/vnd.dxf",
        1805u16 => "image/vnd.fastbidsheet",
        1806u16 => "image/vnd.fpx",
        1807u16 => "image/vnd.fst",
        1808u16 => "image/vnd.fujixerox.edmics-mmr",
        1809u16 => "image/vnd.fujixerox.edmics-rlc",
        1810u16 => "image/vnd.globalgraphics.pgb",
        1811u16 => "image/vnd.microsoft.icon",
        1812u16 => "image/vnd.mix",
        1813u16 => "image/vnd.mozilla.apng",
        1814u16 => "image/vnd.ms-modi",
        1815u16 => "image/vnd.net-fpx",
        1816u16 => "image/vnd.pco.b16",
        1817u16 => "image/vnd.radiance",
        1818u16 => "image/vnd.sealed.png",
        1819u16 => "image/vnd.sealedmedia.softseal.gif",
        1820u16 => "image/vnd.sealedmedia.softseal.jpg",
        1821u16 => "image/vnd.svf",
        1822u16 => "image/vnd.tencent.tap",
        1823u16 => "image/vnd.valve.source.texture",
        1824u16 => "image/vnd.wap.wbmp",
        1825u16 => "image/vnd.xiff",
        1826u16 => "image/vnd.zbrush.pcx",
        1827u16 => "image/webp",
        1828u16 => "image/wmf",
        1829u16 => "message/CPIM",
        1830u16 => "message/bhttp",
        1831u16 => "message/delivery-status",
        1832u16 => "message/disposition-notification",
        1833u16 => "message/example",
        1834u16 => "message/external-body",
        1835u16 => "message/feedback-report",
        1836u16 => "message/global",
        1837u16 => "message/global-delivery-status",
        1838u16 => "message/global-disposition-notification",
        1839u16 => "message/global-headers",
        1840u16 => "message/http",
        1841u16 => "message/imdn+xml",
        1842u16 => "message/mls",
        1843u16 => "message/news",
        1844u16 => "message/ohttp-req",
        1845u16 => "message/ohttp-res",
        1846u16 => "message/partial",
        1847u16 => "message/rfc822",
        1848u16 => "message/s-http",
        1849u16 => "message/sip",
        1850u16 => "message/sipfrag",
        1851u16 => "message/tracking-status",
        1852u16 => "message/vnd.si.simp",
        1853u16 => "message/vnd.wfa.wsc",
        1854u16 => "model/3mf",
        1855u16 => "model/JT",
        1856u16 => "model/e57",
        1857u16 => "model/example",
        1858u16 => "model/gltf+json",
        1859u16 => "model/gltf-binary",
        1860u16 => "model/iges",
        1861u16 => "model/mesh",
        1862u16 => "model/mtl",
        1863u16 => "model/obj",
        1864u16 => "model/prc",
        1865u16 => "model/step",
        1866u16 => "model/step+xml",
        1867u16 => "model/step+zip",
        1868u16 => "model/step-xml+zip",
        1869u16 => "model/stl",
        1870u16 => "model/u3d",
        1871u16 => "model/vnd.bary",
        1872u16 => "model/vnd.cld",
        1873u16 => "model/vnd.collada+xml",
        1874u16 => "model/vnd.dwf",
        1875u16 => "model/vnd.flatland.3dml",
        1876u16 => "model/vnd.gdl",
        1877u16 => "model/vnd.gs-gdl",
        1878u16 => "model/vnd.gtw",
        1879u16 => "model/vnd.moml+xml",
        1880u16 => "model/vnd.mts",
        1881u16 => "model/vnd.opengex",
        1882u16 => "model/vnd.parasolid.transmit.binary",
        1883u16 => "model/vnd.parasolid.transmit.text",
        1884u16 => "model/vnd.pytha.pyox",
        1885u16 => "model/vnd.rosette.annotated-data-model",
        1886u16 => "model/vnd.sap.vds",
        1887u16 => "model/vnd.usda",
        1888u16 => "model/vnd.usdz+zip",
        1889u16 => "model/vnd.valve.source.compiled-map",
        1890u16 => "model/vnd.vtu",
        1891u16 => "model/vrml",
        1892u16 => "model/x3d+fastinfoset",
        1893u16 => "model/x3d+xml",
        1894u16 => "model/x3d-vrml",
        1895u16 => "multipart/alternative",
        1896u16 => "multipart/appledouble",
        1897u16 => "multipart/byteranges",
        1898u16 => "multipart/digest",
        1899u16 => "multipart/encrypted",
        1900u16 => "multipart/example",
        1901u16 => "multipart/form-data",
        1902u16 => "multipart/header-set",
        1903u16 => "multipart/mixed",
        1904u16 => "multipart/multilingual",
        1905u16 => "multipart/parallel",
        1906u16 => "multipart/related",
        1907u16 => "multipart/report",
        1908u16 => "multipart/signed",
        1909u16 => "multipart/vnd.bint.med-plus",
        1910u16 => "multipart/voice-message",
        1911u16 => "multipart/x-mixed-replace",
        1912u16 => "text/1d-interleaved-parityfec",
        1913u16 => "text/RED",
        1914u16 => "text/SGML",
        1915u16 => "text/cache-manifest",
        1916u16 => "text/calendar",
        1917u16 => "text/cql",
        1918u16 => "text/cql-expression",
        1919u16 => "text/cql-identifier",
        1920u16 => "text/css",
        1921u16 => "text/csv",
        1922u16 => "text/csv-schema",
        1923u16 => "text/directory",
        1924u16 => "text/dns",
        1925u16 => "text/ecmascript",
        1926u16 => "text/encaprtp",
        1927u16 => "text/enriched",
        1928u16 => "text/example",
        1929u16 => "text/fhirpath",
        1930u16 => "text/flexfec",
        1931u16 => "text/fwdred",
        1932u16 => "text/gff3",
        1933u16 => "text/grammar-ref-list",
        1934u16 => "text/hl7v2",
        1935u16 => "text/html",
        1936u16 => "text/javascript",
        1937u16 => "text/jcr-cnd",
        1938u16 => "text/markdown",
        1939u16 => "text/mizar",
        1940u16 => "text/n3",
        1941u16 => "text/parameters",
        1942u16 => "text/parityfec",
        1943u16 => "text/plain",
        1944u16 => "text/provenance-notation",
        1945u16 => "text/prs.fallenstein.rst",
        1946u16 => "text/prs.lines.tag",
        1947u16 => "text/prs.prop.logic",
        1948u16 => "text/prs.texi",
        1949u16 => "text/raptorfec",
        1950u16 => "text/rfc822-headers",
        1951u16 => "text/richtext",
        1952u16 => "text/rtf",
        1953u16 => "text/rtp-enc-aescm128",
        1954u16 => "text/rtploopback",
        1955u16 => "text/rtx",
        1956u16 => "text/shaclc",
        1957u16 => "text/shex",
        1958u16 => "text/spdx",
        1959u16 => "text/strings",
        1960u16 => "text/t140",
        1961u16 => "text/tab-separated-values",
        1962u16 => "text/troff",
        1963u16 => "text/turtle",
        1964u16 => "text/ulpfec",
        1965u16 => "text/uri-list",
        1966u16 => "text/vcard",
        1967u16 => "text/vnd.DMClientScript",
        1968u16 => "text/vnd.IPTC.NITF",
        1969u16 => "text/vnd.IPTC.NewsML",
        1970u16 => "text/vnd.a",
        1971u16 => "text/vnd.abc",
        1972u16 => "text/vnd.ascii-art",
        1973u16 => "text/vnd.curl",
        1974u16 => "text/vnd.debian.copyright",
        1975u16 => "text/vnd.dvb.subtitle",
        1976u16 => "text/vnd.esmertec.theme-descriptor",
        1977u16 => "text/vnd.exchangeable",
        1978u16 => "text/vnd.familysearch.gedcom",
        1979u16 => "text/vnd.ficlab.flt",
        1980u16 => "text/vnd.fly",
        1981u16 => "text/vnd.fmi.flexstor",
        1982u16 => "text/vnd.gml",
        1983u16 => "text/vnd.graphviz",
        1984u16 => "text/vnd.hans",
        1985u16 => "text/vnd.hgl",
        1986u16 => "text/vnd.in3d.3dml",
        1987u16 => "text/vnd.in3d.spot",
        1988u16 => "text/vnd.latex-z",
        1989u16 => "text/vnd.motorola.reflex",
        1990u16 => "text/vnd.ms-mediapackage",
        1991u16 => "text/vnd.net2phone.commcenter.command",
        1992u16 => "text/vnd.radisys.msml-basic-layout",
        1993u16 => "text/vnd.senx.warpscript",
        1994u16 => "text/vnd.si.uricatalogue",
        1995u16 => "text/vnd.sosi",
        1996u16 => "text/vnd.sun.j2me.app-descriptor",
        1997u16 => "text/vnd.trolltech.linguist",
        1998u16 => "text/vnd.wap.si",
        1999u16 => "text/vnd.wap.sl",
        2000u16 => "text/vnd.wap.wml",
        2001u16 => "text/vnd.wap.wmlscript",
        2002u16 => "text/vtt",
        2003u16 => "text/wgsl",
        2004u16 => "text/xml",
        2005u16 => "text/xml-external-parsed-entity",
        2006u16 => "video/1d-interleaved-parityfec",
        2007u16 => "video/3gpp",
        2008u16 => "video/3gpp-tt",
        2009u16 => "video/3gpp2",
        2010u16 => "video/AV1",
        2011u16 => "video/BMPEG",
        2012u16 => "video/BT656",
        2013u16 => "video/CelB",
        2014u16 => "video/DV",
        2015u16 => "video/FFV1",
        2016u16 => "video/H261",
        2017u16 => "video/H263",
        2018u16 => "video/H263-1998",
        2019u16 => "video/H263-2000",
        2020u16 => "video/H264",
        2021u16 => "video/H264-RCDO",
        2022u16 => "video/H264-SVC",
        2023u16 => "video/H265",
        2024u16 => "video/H266",
        2025u16 => "video/JPEG",
        2026u16 => "video/MP1S",
        2027u16 => "video/MP2P",
        2028u16 => "video/MP2T",
        2029u16 => "video/MP4V-ES",
        2030u16 => "video/MPV",
        2031u16 => "video/SMPTE292M",
        2032u16 => "video/VP8",
        2033u16 => "video/VP9",
        2034u16 => "video/encaprtp",
        2035u16 => "video/evc",
        2036u16 => "video/example",
        2037u16 => "video/flexfec",
        2038u16 => "video/iso.segment",
        2039u16 => "video/jpeg2000",
        2040u16 => "video/jxsv",
        2041u16 => "video/matroska",
        2042u16 => "video/matroska-3d",
        2043u16 => "video/mj2",
        2044u16 => "video/mp4",
        2045u16 => "video/mpeg",
        2046u16 => "video/mpeg4-generic",
        2047u16 => "video/nv",
        2048u16 => "video/ogg",
        2049u16 => "video/parityfec",
        2050u16 => "video/pointer",
        2051u16 => "video/quicktime",
        2052u16 => "video/raptorfec",
        2053u16 => "video/raw",
        2054u16 => "video/rtp-enc-aescm128",
        2055u16 => "video/rtploopback",
        2056u16 => "video/rtx",
        2057u16 => "video/scip",
        2058u16 => "video/smpte291",
        2059u16 => "video/ulpfec",
        2060u16 => "video/vc1",
        2061u16 => "video/vc2",
        2062u16 => "video/vnd.CCTV",
        2063u16 => "video/vnd.dece.hd",
        2064u16 => "video/vnd.dece.mobile",
        2065u16 => "video/vnd.dece.mp4",
        2066u16 => "video/vnd.dece.pd",
        2067u16 => "video/vnd.dece.sd",
        2068u16 => "video/vnd.dece.video",
        2069u16 => "video/vnd.directv.mpeg",
        2070u16 => "video/vnd.directv.mpeg-tts",
        2071u16 => "video/vnd.dlna.mpeg-tts",
        2072u16 => "video/vnd.dvb.file",
        2073u16 => "video/vnd.fvt",
        2074u16 => "video/vnd.hns.video",
        2075u16 => "video/vnd.iptvforum.1dparityfec-1010",
        2076u16 => "video/vnd.iptvforum.1dparityfec-2005",
        2077u16 => "video/vnd.iptvforum.2dparityfec-1010",
        2078u16 => "video/vnd.iptvforum.2dparityfec-2005",
        2079u16 => "video/vnd.iptvforum.ttsavc",
        2080u16 => "video/vnd.iptvforum.ttsmpeg2",
        2081u16 => "video/vnd.motorola.video",
        2082u16 => "video/vnd.motorola.videop",
        2083u16 => "video/vnd.mpegurl",
        2084u16 => "video/vnd.ms-playready.media.pyv",
        2085u16 => "video/vnd.nokia.interleaved-multimedia",
        2086u16 => "video/vnd.nokia.mp4vr",
        2087u16 => "video/vnd.nokia.videovoip",
        2088u16 => "video/vnd.objectvideo",
        2089u16 => "video/vnd.radgamettools.bink",
        2090u16 => "video/vnd.radgamettools.smacker",
        2091u16 => "video/vnd.sealed.mpeg1",
        2092u16 => "video/vnd.sealed.mpeg4",
        2093u16 => "video/vnd.sealed.swf",
        2094u16 => "video/vnd.sealedmedia.softseal.mov",
        2095u16 => "video/vnd.uvvu.mp4",
        2096u16 => "video/vnd.vivo",
        2097u16 => "video/vnd.youtube.yt",
    };

    pub(super) const KNOWN_STRING: phf::OrderedMap<&'static str, EncodingPrefix> = phf_ordered_map! {
        "empty" => 0u16,
        "application/1d-interleaved-parityfec" => 1u16,
        "application/3gpdash-qoe-report+xml" => 2u16,
        "application/3gpp-ims+xml" => 3u16,
        "application/3gppHal+json" => 4u16,
        "application/3gppHalForms+json" => 5u16,
        "application/A2L" => 6u16,
        "application/AML" => 7u16,
        "application/ATF" => 8u16,
        "application/ATFX" => 9u16,
        "application/ATXML" => 10u16,
        "application/CALS-1840" => 11u16,
        "application/CDFX+XML" => 12u16,
        "application/CEA" => 13u16,
        "application/CSTAdata+xml" => 14u16,
        "application/DCD" => 15u16,
        "application/DII" => 16u16,
        "application/DIT" => 17u16,
        "application/EDI-X12" => 18u16,
        "application/EDI-consent" => 19u16,
        "application/EDIFACT" => 20u16,
        "application/EmergencyCallData.Comment+xml" => 21u16,
        "application/EmergencyCallData.Control+xml" => 22u16,
        "application/EmergencyCallData.DeviceInfo+xml" => 23u16,
        "application/EmergencyCallData.LegacyESN+json" => 24u16,
        "application/EmergencyCallData.ProviderInfo+xml" => 25u16,
        "application/EmergencyCallData.ServiceInfo+xml" => 26u16,
        "application/EmergencyCallData.SubscriberInfo+xml" => 27u16,
        "application/EmergencyCallData.VEDS+xml" => 28u16,
        "application/EmergencyCallData.cap+xml" => 29u16,
        "application/EmergencyCallData.eCall.MSD" => 30u16,
        "application/H224" => 31u16,
        "application/IOTP" => 32u16,
        "application/ISUP" => 33u16,
        "application/LXF" => 34u16,
        "application/MF4" => 35u16,
        "application/ODA" => 36u16,
        "application/ODX" => 37u16,
        "application/PDX" => 38u16,
        "application/QSIG" => 39u16,
        "application/SGML" => 40u16,
        "application/TETRA_ISI" => 41u16,
        "application/ace+cbor" => 42u16,
        "application/ace+json" => 43u16,
        "application/activemessage" => 44u16,
        "application/activity+json" => 45u16,
        "application/aif+cbor" => 46u16,
        "application/aif+json" => 47u16,
        "application/alto-cdni+json" => 48u16,
        "application/alto-cdnifilter+json" => 49u16,
        "application/alto-costmap+json" => 50u16,
        "application/alto-costmapfilter+json" => 51u16,
        "application/alto-directory+json" => 52u16,
        "application/alto-endpointcost+json" => 53u16,
        "application/alto-endpointcostparams+json" => 54u16,
        "application/alto-endpointprop+json" => 55u16,
        "application/alto-endpointpropparams+json" => 56u16,
        "application/alto-error+json" => 57u16,
        "application/alto-networkmap+json" => 58u16,
        "application/alto-networkmapfilter+json" => 59u16,
        "application/alto-propmap+json" => 60u16,
        "application/alto-propmapparams+json" => 61u16,
        "application/alto-tips+json" => 62u16,
        "application/alto-tipsparams+json" => 63u16,
        "application/alto-updatestreamcontrol+json" => 64u16,
        "application/alto-updatestreamparams+json" => 65u16,
        "application/andrew-inset" => 66u16,
        "application/applefile" => 67u16,
        "application/at+jwt" => 68u16,
        "application/atom+xml" => 69u16,
        "application/atomcat+xml" => 70u16,
        "application/atomdeleted+xml" => 71u16,
        "application/atomicmail" => 72u16,
        "application/atomsvc+xml" => 73u16,
        "application/atsc-dwd+xml" => 74u16,
        "application/atsc-dynamic-event-message" => 75u16,
        "application/atsc-held+xml" => 76u16,
        "application/atsc-rdt+json" => 77u16,
        "application/atsc-rsat+xml" => 78u16,
        "application/auth-policy+xml" => 79u16,
        "application/automationml-aml+xml" => 80u16,
        "application/automationml-amlx+zip" => 81u16,
        "application/bacnet-xdd+zip" => 82u16,
        "application/batch-SMTP" => 83u16,
        "application/beep+xml" => 84u16,
        "application/c2pa" => 85u16,
        "application/calendar+json" => 86u16,
        "application/calendar+xml" => 87u16,
        "application/call-completion" => 88u16,
        "application/captive+json" => 89u16,
        "application/cbor" => 90u16,
        "application/cbor-seq" => 91u16,
        "application/cccex" => 92u16,
        "application/ccmp+xml" => 93u16,
        "application/ccxml+xml" => 94u16,
        "application/cda+xml" => 95u16,
        "application/cdmi-capability" => 96u16,
        "application/cdmi-container" => 97u16,
        "application/cdmi-domain" => 98u16,
        "application/cdmi-object" => 99u16,
        "application/cdmi-queue" => 100u16,
        "application/cdni" => 101u16,
        "application/cea-2018+xml" => 102u16,
        "application/cellml+xml" => 103u16,
        "application/cfw" => 104u16,
        "application/cid-edhoc+cbor-seq" => 105u16,
        "application/city+json" => 106u16,
        "application/clr" => 107u16,
        "application/clue+xml" => 108u16,
        "application/clue_info+xml" => 109u16,
        "application/cms" => 110u16,
        "application/cnrp+xml" => 111u16,
        "application/coap-group+json" => 112u16,
        "application/coap-payload" => 113u16,
        "application/commonground" => 114u16,
        "application/concise-problem-details+cbor" => 115u16,
        "application/conference-info+xml" => 116u16,
        "application/cose" => 117u16,
        "application/cose-key" => 118u16,
        "application/cose-key-set" => 119u16,
        "application/cose-x509" => 120u16,
        "application/cpl+xml" => 121u16,
        "application/csrattrs" => 122u16,
        "application/csta+xml" => 123u16,
        "application/csvm+json" => 124u16,
        "application/cwl" => 125u16,
        "application/cwl+json" => 126u16,
        "application/cwt" => 127u16,
        "application/cybercash" => 128u16,
        "application/dash+xml" => 129u16,
        "application/dash-patch+xml" => 130u16,
        "application/dashdelta" => 131u16,
        "application/davmount+xml" => 132u16,
        "application/dca-rft" => 133u16,
        "application/dec-dx" => 134u16,
        "application/dialog-info+xml" => 135u16,
        "application/dicom" => 136u16,
        "application/dicom+json" => 137u16,
        "application/dicom+xml" => 138u16,
        "application/dns" => 139u16,
        "application/dns+json" => 140u16,
        "application/dns-message" => 141u16,
        "application/dots+cbor" => 142u16,
        "application/dpop+jwt" => 143u16,
        "application/dskpp+xml" => 144u16,
        "application/dssc+der" => 145u16,
        "application/dssc+xml" => 146u16,
        "application/dvcs" => 147u16,
        "application/ecmascript" => 148u16,
        "application/edhoc+cbor-seq" => 149u16,
        "application/efi" => 150u16,
        "application/elm+json" => 151u16,
        "application/elm+xml" => 152u16,
        "application/emma+xml" => 153u16,
        "application/emotionml+xml" => 154u16,
        "application/encaprtp" => 155u16,
        "application/epp+xml" => 156u16,
        "application/epub+zip" => 157u16,
        "application/eshop" => 158u16,
        "application/example" => 159u16,
        "application/exi" => 160u16,
        "application/expect-ct-report+json" => 161u16,
        "application/express" => 162u16,
        "application/fastinfoset" => 163u16,
        "application/fastsoap" => 164u16,
        "application/fdf" => 165u16,
        "application/fdt+xml" => 166u16,
        "application/fhir+json" => 167u16,
        "application/fhir+xml" => 168u16,
        "application/fits" => 169u16,
        "application/flexfec" => 170u16,
        "application/font-sfnt" => 171u16,
        "application/font-tdpfr" => 172u16,
        "application/font-woff" => 173u16,
        "application/framework-attributes+xml" => 174u16,
        "application/geo+json" => 175u16,
        "application/geo+json-seq" => 176u16,
        "application/geopackage+sqlite3" => 177u16,
        "application/geoxacml+json" => 178u16,
        "application/geoxacml+xml" => 179u16,
        "application/gltf-buffer" => 180u16,
        "application/gml+xml" => 181u16,
        "application/gzip" => 182u16,
        "application/held+xml" => 183u16,
        "application/hl7v2+xml" => 184u16,
        "application/http" => 185u16,
        "application/hyperstudio" => 186u16,
        "application/ibe-key-request+xml" => 187u16,
        "application/ibe-pkg-reply+xml" => 188u16,
        "application/ibe-pp-data" => 189u16,
        "application/iges" => 190u16,
        "application/im-iscomposing+xml" => 191u16,
        "application/index" => 192u16,
        "application/index.cmd" => 193u16,
        "application/index.obj" => 194u16,
        "application/index.response" => 195u16,
        "application/index.vnd" => 196u16,
        "application/inkml+xml" => 197u16,
        "application/ipfix" => 198u16,
        "application/ipp" => 199u16,
        "application/its+xml" => 200u16,
        "application/java-archive" => 201u16,
        "application/javascript" => 202u16,
        "application/jf2feed+json" => 203u16,
        "application/jose" => 204u16,
        "application/jose+json" => 205u16,
        "application/jrd+json" => 206u16,
        "application/jscalendar+json" => 207u16,
        "application/jscontact+json" => 208u16,
        "application/json" => 209u16,
        "application/json-patch+json" => 210u16,
        "application/json-seq" => 211u16,
        "application/jsonpath" => 212u16,
        "application/jwk+json" => 213u16,
        "application/jwk-set+json" => 214u16,
        "application/jwt" => 215u16,
        "application/kpml-request+xml" => 216u16,
        "application/kpml-response+xml" => 217u16,
        "application/ld+json" => 218u16,
        "application/lgr+xml" => 219u16,
        "application/link-format" => 220u16,
        "application/linkset" => 221u16,
        "application/linkset+json" => 222u16,
        "application/load-control+xml" => 223u16,
        "application/logout+jwt" => 224u16,
        "application/lost+xml" => 225u16,
        "application/lostsync+xml" => 226u16,
        "application/lpf+zip" => 227u16,
        "application/mac-binhex40" => 228u16,
        "application/macwriteii" => 229u16,
        "application/mads+xml" => 230u16,
        "application/manifest+json" => 231u16,
        "application/marc" => 232u16,
        "application/marcxml+xml" => 233u16,
        "application/mathematica" => 234u16,
        "application/mathml+xml" => 235u16,
        "application/mathml-content+xml" => 236u16,
        "application/mathml-presentation+xml" => 237u16,
        "application/mbms-associated-procedure-description+xml" => 238u16,
        "application/mbms-deregister+xml" => 239u16,
        "application/mbms-envelope+xml" => 240u16,
        "application/mbms-msk+xml" => 241u16,
        "application/mbms-msk-response+xml" => 242u16,
        "application/mbms-protection-description+xml" => 243u16,
        "application/mbms-reception-report+xml" => 244u16,
        "application/mbms-register+xml" => 245u16,
        "application/mbms-register-response+xml" => 246u16,
        "application/mbms-schedule+xml" => 247u16,
        "application/mbms-user-service-description+xml" => 248u16,
        "application/mbox" => 249u16,
        "application/media-policy-dataset+xml" => 250u16,
        "application/media_control+xml" => 251u16,
        "application/mediaservercontrol+xml" => 252u16,
        "application/merge-patch+json" => 253u16,
        "application/metalink4+xml" => 254u16,
        "application/mets+xml" => 255u16,
        "application/mikey" => 256u16,
        "application/mipc" => 257u16,
        "application/missing-blocks+cbor-seq" => 258u16,
        "application/mmt-aei+xml" => 259u16,
        "application/mmt-usd+xml" => 260u16,
        "application/mods+xml" => 261u16,
        "application/moss-keys" => 262u16,
        "application/moss-signature" => 263u16,
        "application/mosskey-data" => 264u16,
        "application/mosskey-request" => 265u16,
        "application/mp21" => 266u16,
        "application/mp4" => 267u16,
        "application/mpeg4-generic" => 268u16,
        "application/mpeg4-iod" => 269u16,
        "application/mpeg4-iod-xmt" => 270u16,
        "application/mrb-consumer+xml" => 271u16,
        "application/mrb-publish+xml" => 272u16,
        "application/msc-ivr+xml" => 273u16,
        "application/msc-mixer+xml" => 274u16,
        "application/msword" => 275u16,
        "application/mud+json" => 276u16,
        "application/multipart-core" => 277u16,
        "application/mxf" => 278u16,
        "application/n-quads" => 279u16,
        "application/n-triples" => 280u16,
        "application/nasdata" => 281u16,
        "application/news-checkgroups" => 282u16,
        "application/news-groupinfo" => 283u16,
        "application/news-transmission" => 284u16,
        "application/nlsml+xml" => 285u16,
        "application/node" => 286u16,
        "application/nss" => 287u16,
        "application/oauth-authz-req+jwt" => 288u16,
        "application/oblivious-dns-message" => 289u16,
        "application/ocsp-request" => 290u16,
        "application/ocsp-response" => 291u16,
        "application/octet-stream" => 292u16,
        "application/odm+xml" => 293u16,
        "application/oebps-package+xml" => 294u16,
        "application/ogg" => 295u16,
        "application/ohttp-keys" => 296u16,
        "application/opc-nodeset+xml" => 297u16,
        "application/oscore" => 298u16,
        "application/oxps" => 299u16,
        "application/p21" => 300u16,
        "application/p21+zip" => 301u16,
        "application/p2p-overlay+xml" => 302u16,
        "application/parityfec" => 303u16,
        "application/passport" => 304u16,
        "application/patch-ops-error+xml" => 305u16,
        "application/pdf" => 306u16,
        "application/pem-certificate-chain" => 307u16,
        "application/pgp-encrypted" => 308u16,
        "application/pgp-keys" => 309u16,
        "application/pgp-signature" => 310u16,
        "application/pidf+xml" => 311u16,
        "application/pidf-diff+xml" => 312u16,
        "application/pkcs10" => 313u16,
        "application/pkcs12" => 314u16,
        "application/pkcs7-mime" => 315u16,
        "application/pkcs7-signature" => 316u16,
        "application/pkcs8" => 317u16,
        "application/pkcs8-encrypted" => 318u16,
        "application/pkix-attr-cert" => 319u16,
        "application/pkix-cert" => 320u16,
        "application/pkix-crl" => 321u16,
        "application/pkix-pkipath" => 322u16,
        "application/pkixcmp" => 323u16,
        "application/pls+xml" => 324u16,
        "application/poc-settings+xml" => 325u16,
        "application/postscript" => 326u16,
        "application/ppsp-tracker+json" => 327u16,
        "application/private-token-issuer-directory" => 328u16,
        "application/private-token-request" => 329u16,
        "application/private-token-response" => 330u16,
        "application/problem+json" => 331u16,
        "application/problem+xml" => 332u16,
        "application/provenance+xml" => 333u16,
        "application/prs.alvestrand.titrax-sheet" => 334u16,
        "application/prs.cww" => 335u16,
        "application/prs.cyn" => 336u16,
        "application/prs.hpub+zip" => 337u16,
        "application/prs.implied-document+xml" => 338u16,
        "application/prs.implied-executable" => 339u16,
        "application/prs.implied-object+json" => 340u16,
        "application/prs.implied-object+json-seq" => 341u16,
        "application/prs.implied-object+yaml" => 342u16,
        "application/prs.implied-structure" => 343u16,
        "application/prs.nprend" => 344u16,
        "application/prs.plucker" => 345u16,
        "application/prs.rdf-xml-crypt" => 346u16,
        "application/prs.vcfbzip2" => 347u16,
        "application/prs.xsf+xml" => 348u16,
        "application/pskc+xml" => 349u16,
        "application/pvd+json" => 350u16,
        "application/raptorfec" => 351u16,
        "application/rdap+json" => 352u16,
        "application/rdf+xml" => 353u16,
        "application/reginfo+xml" => 354u16,
        "application/relax-ng-compact-syntax" => 355u16,
        "application/remote-printing" => 356u16,
        "application/reputon+json" => 357u16,
        "application/resource-lists+xml" => 358u16,
        "application/resource-lists-diff+xml" => 359u16,
        "application/rfc+xml" => 360u16,
        "application/riscos" => 361u16,
        "application/rlmi+xml" => 362u16,
        "application/rls-services+xml" => 363u16,
        "application/route-apd+xml" => 364u16,
        "application/route-s-tsid+xml" => 365u16,
        "application/route-usd+xml" => 366u16,
        "application/rpki-checklist" => 367u16,
        "application/rpki-ghostbusters" => 368u16,
        "application/rpki-manifest" => 369u16,
        "application/rpki-publication" => 370u16,
        "application/rpki-roa" => 371u16,
        "application/rpki-updown" => 372u16,
        "application/rtf" => 373u16,
        "application/rtploopback" => 374u16,
        "application/rtx" => 375u16,
        "application/samlassertion+xml" => 376u16,
        "application/samlmetadata+xml" => 377u16,
        "application/sarif+json" => 378u16,
        "application/sarif-external-properties+json" => 379u16,
        "application/sbe" => 380u16,
        "application/sbml+xml" => 381u16,
        "application/scaip+xml" => 382u16,
        "application/scim+json" => 383u16,
        "application/scvp-cv-request" => 384u16,
        "application/scvp-cv-response" => 385u16,
        "application/scvp-vp-request" => 386u16,
        "application/scvp-vp-response" => 387u16,
        "application/sdp" => 388u16,
        "application/secevent+jwt" => 389u16,
        "application/senml+cbor" => 390u16,
        "application/senml+json" => 391u16,
        "application/senml+xml" => 392u16,
        "application/senml-etch+cbor" => 393u16,
        "application/senml-etch+json" => 394u16,
        "application/senml-exi" => 395u16,
        "application/sensml+cbor" => 396u16,
        "application/sensml+json" => 397u16,
        "application/sensml+xml" => 398u16,
        "application/sensml-exi" => 399u16,
        "application/sep+xml" => 400u16,
        "application/sep-exi" => 401u16,
        "application/session-info" => 402u16,
        "application/set-payment" => 403u16,
        "application/set-payment-initiation" => 404u16,
        "application/set-registration" => 405u16,
        "application/set-registration-initiation" => 406u16,
        "application/sgml-open-catalog" => 407u16,
        "application/shf+xml" => 408u16,
        "application/sieve" => 409u16,
        "application/simple-filter+xml" => 410u16,
        "application/simple-message-summary" => 411u16,
        "application/simpleSymbolContainer" => 412u16,
        "application/sipc" => 413u16,
        "application/slate" => 414u16,
        "application/smil" => 415u16,
        "application/smil+xml" => 416u16,
        "application/smpte336m" => 417u16,
        "application/soap+fastinfoset" => 418u16,
        "application/soap+xml" => 419u16,
        "application/sparql-query" => 420u16,
        "application/sparql-results+xml" => 421u16,
        "application/spdx+json" => 422u16,
        "application/spirits-event+xml" => 423u16,
        "application/sql" => 424u16,
        "application/srgs" => 425u16,
        "application/srgs+xml" => 426u16,
        "application/sru+xml" => 427u16,
        "application/ssml+xml" => 428u16,
        "application/stix+json" => 429u16,
        "application/swid+cbor" => 430u16,
        "application/swid+xml" => 431u16,
        "application/tamp-apex-update" => 432u16,
        "application/tamp-apex-update-confirm" => 433u16,
        "application/tamp-community-update" => 434u16,
        "application/tamp-community-update-confirm" => 435u16,
        "application/tamp-error" => 436u16,
        "application/tamp-sequence-adjust" => 437u16,
        "application/tamp-sequence-adjust-confirm" => 438u16,
        "application/tamp-status-query" => 439u16,
        "application/tamp-status-response" => 440u16,
        "application/tamp-update" => 441u16,
        "application/tamp-update-confirm" => 442u16,
        "application/taxii+json" => 443u16,
        "application/td+json" => 444u16,
        "application/tei+xml" => 445u16,
        "application/thraud+xml" => 446u16,
        "application/timestamp-query" => 447u16,
        "application/timestamp-reply" => 448u16,
        "application/timestamped-data" => 449u16,
        "application/tlsrpt+gzip" => 450u16,
        "application/tlsrpt+json" => 451u16,
        "application/tm+json" => 452u16,
        "application/tnauthlist" => 453u16,
        "application/token-introspection+jwt" => 454u16,
        "application/trickle-ice-sdpfrag" => 455u16,
        "application/trig" => 456u16,
        "application/ttml+xml" => 457u16,
        "application/tve-trigger" => 458u16,
        "application/tzif" => 459u16,
        "application/tzif-leap" => 460u16,
        "application/ulpfec" => 461u16,
        "application/urc-grpsheet+xml" => 462u16,
        "application/urc-ressheet+xml" => 463u16,
        "application/urc-targetdesc+xml" => 464u16,
        "application/urc-uisocketdesc+xml" => 465u16,
        "application/vcard+json" => 466u16,
        "application/vcard+xml" => 467u16,
        "application/vemmi" => 468u16,
        "application/vnd.1000minds.decision-model+xml" => 469u16,
        "application/vnd.1ob" => 470u16,
        "application/vnd.3M.Post-it-Notes" => 471u16,
        "application/vnd.3gpp-prose+xml" => 472u16,
        "application/vnd.3gpp-prose-pc3a+xml" => 473u16,
        "application/vnd.3gpp-prose-pc3ach+xml" => 474u16,
        "application/vnd.3gpp-prose-pc3ch+xml" => 475u16,
        "application/vnd.3gpp-prose-pc8+xml" => 476u16,
        "application/vnd.3gpp-v2x-local-service-information" => 477u16,
        "application/vnd.3gpp.5gnas" => 478u16,
        "application/vnd.3gpp.GMOP+xml" => 479u16,
        "application/vnd.3gpp.SRVCC-info+xml" => 480u16,
        "application/vnd.3gpp.access-transfer-events+xml" => 481u16,
        "application/vnd.3gpp.bsf+xml" => 482u16,
        "application/vnd.3gpp.crs+xml" => 483u16,
        "application/vnd.3gpp.current-location-discovery+xml" => 484u16,
        "application/vnd.3gpp.gtpc" => 485u16,
        "application/vnd.3gpp.interworking-data" => 486u16,
        "application/vnd.3gpp.lpp" => 487u16,
        "application/vnd.3gpp.mc-signalling-ear" => 488u16,
        "application/vnd.3gpp.mcdata-affiliation-command+xml" => 489u16,
        "application/vnd.3gpp.mcdata-info+xml" => 490u16,
        "application/vnd.3gpp.mcdata-msgstore-ctrl-request+xml" => 491u16,
        "application/vnd.3gpp.mcdata-payload" => 492u16,
        "application/vnd.3gpp.mcdata-regroup+xml" => 493u16,
        "application/vnd.3gpp.mcdata-service-config+xml" => 494u16,
        "application/vnd.3gpp.mcdata-signalling" => 495u16,
        "application/vnd.3gpp.mcdata-ue-config+xml" => 496u16,
        "application/vnd.3gpp.mcdata-user-profile+xml" => 497u16,
        "application/vnd.3gpp.mcptt-affiliation-command+xml" => 498u16,
        "application/vnd.3gpp.mcptt-floor-request+xml" => 499u16,
        "application/vnd.3gpp.mcptt-info+xml" => 500u16,
        "application/vnd.3gpp.mcptt-location-info+xml" => 501u16,
        "application/vnd.3gpp.mcptt-mbms-usage-info+xml" => 502u16,
        "application/vnd.3gpp.mcptt-regroup+xml" => 503u16,
        "application/vnd.3gpp.mcptt-service-config+xml" => 504u16,
        "application/vnd.3gpp.mcptt-signed+xml" => 505u16,
        "application/vnd.3gpp.mcptt-ue-config+xml" => 506u16,
        "application/vnd.3gpp.mcptt-ue-init-config+xml" => 507u16,
        "application/vnd.3gpp.mcptt-user-profile+xml" => 508u16,
        "application/vnd.3gpp.mcvideo-affiliation-command+xml" => 509u16,
        "application/vnd.3gpp.mcvideo-affiliation-info+xml" => 510u16,
        "application/vnd.3gpp.mcvideo-info+xml" => 511u16,
        "application/vnd.3gpp.mcvideo-location-info+xml" => 512u16,
        "application/vnd.3gpp.mcvideo-mbms-usage-info+xml" => 513u16,
        "application/vnd.3gpp.mcvideo-regroup+xml" => 514u16,
        "application/vnd.3gpp.mcvideo-service-config+xml" => 515u16,
        "application/vnd.3gpp.mcvideo-transmission-request+xml" => 516u16,
        "application/vnd.3gpp.mcvideo-ue-config+xml" => 517u16,
        "application/vnd.3gpp.mcvideo-user-profile+xml" => 518u16,
        "application/vnd.3gpp.mid-call+xml" => 519u16,
        "application/vnd.3gpp.ngap" => 520u16,
        "application/vnd.3gpp.pfcp" => 521u16,
        "application/vnd.3gpp.pic-bw-large" => 522u16,
        "application/vnd.3gpp.pic-bw-small" => 523u16,
        "application/vnd.3gpp.pic-bw-var" => 524u16,
        "application/vnd.3gpp.s1ap" => 525u16,
        "application/vnd.3gpp.seal-group-doc+xml" => 526u16,
        "application/vnd.3gpp.seal-info+xml" => 527u16,
        "application/vnd.3gpp.seal-location-info+xml" => 528u16,
        "application/vnd.3gpp.seal-mbms-usage-info+xml" => 529u16,
        "application/vnd.3gpp.seal-network-QoS-management-info+xml" => 530u16,
        "application/vnd.3gpp.seal-ue-config-info+xml" => 531u16,
        "application/vnd.3gpp.seal-unicast-info+xml" => 532u16,
        "application/vnd.3gpp.seal-user-profile-info+xml" => 533u16,
        "application/vnd.3gpp.sms" => 534u16,
        "application/vnd.3gpp.sms+xml" => 535u16,
        "application/vnd.3gpp.srvcc-ext+xml" => 536u16,
        "application/vnd.3gpp.state-and-event-info+xml" => 537u16,
        "application/vnd.3gpp.ussd+xml" => 538u16,
        "application/vnd.3gpp.v2x" => 539u16,
        "application/vnd.3gpp.vae-info+xml" => 540u16,
        "application/vnd.3gpp2.bcmcsinfo+xml" => 541u16,
        "application/vnd.3gpp2.sms" => 542u16,
        "application/vnd.3gpp2.tcap" => 543u16,
        "application/vnd.3lightssoftware.imagescal" => 544u16,
        "application/vnd.FloGraphIt" => 545u16,
        "application/vnd.HandHeld-Entertainment+xml" => 546u16,
        "application/vnd.Kinar" => 547u16,
        "application/vnd.MFER" => 548u16,
        "application/vnd.Mobius.DAF" => 549u16,
        "application/vnd.Mobius.DIS" => 550u16,
        "application/vnd.Mobius.MBK" => 551u16,
        "application/vnd.Mobius.MQY" => 552u16,
        "application/vnd.Mobius.MSL" => 553u16,
        "application/vnd.Mobius.PLC" => 554u16,
        "application/vnd.Mobius.TXF" => 555u16,
        "application/vnd.Quark.QuarkXPress" => 556u16,
        "application/vnd.RenLearn.rlprint" => 557u16,
        "application/vnd.SimTech-MindMapper" => 558u16,
        "application/vnd.accpac.simply.aso" => 559u16,
        "application/vnd.accpac.simply.imp" => 560u16,
        "application/vnd.acm.addressxfer+json" => 561u16,
        "application/vnd.acm.chatbot+json" => 562u16,
        "application/vnd.acucobol" => 563u16,
        "application/vnd.acucorp" => 564u16,
        "application/vnd.adobe.flash.movie" => 565u16,
        "application/vnd.adobe.formscentral.fcdt" => 566u16,
        "application/vnd.adobe.fxp" => 567u16,
        "application/vnd.adobe.partial-upload" => 568u16,
        "application/vnd.adobe.xdp+xml" => 569u16,
        "application/vnd.aether.imp" => 570u16,
        "application/vnd.afpc.afplinedata" => 571u16,
        "application/vnd.afpc.afplinedata-pagedef" => 572u16,
        "application/vnd.afpc.cmoca-cmresource" => 573u16,
        "application/vnd.afpc.foca-charset" => 574u16,
        "application/vnd.afpc.foca-codedfont" => 575u16,
        "application/vnd.afpc.foca-codepage" => 576u16,
        "application/vnd.afpc.modca" => 577u16,
        "application/vnd.afpc.modca-cmtable" => 578u16,
        "application/vnd.afpc.modca-formdef" => 579u16,
        "application/vnd.afpc.modca-mediummap" => 580u16,
        "application/vnd.afpc.modca-objectcontainer" => 581u16,
        "application/vnd.afpc.modca-overlay" => 582u16,
        "application/vnd.afpc.modca-pagesegment" => 583u16,
        "application/vnd.age" => 584u16,
        "application/vnd.ah-barcode" => 585u16,
        "application/vnd.ahead.space" => 586u16,
        "application/vnd.airzip.filesecure.azf" => 587u16,
        "application/vnd.airzip.filesecure.azs" => 588u16,
        "application/vnd.amadeus+json" => 589u16,
        "application/vnd.amazon.mobi8-ebook" => 590u16,
        "application/vnd.americandynamics.acc" => 591u16,
        "application/vnd.amiga.ami" => 592u16,
        "application/vnd.amundsen.maze+xml" => 593u16,
        "application/vnd.android.ota" => 594u16,
        "application/vnd.anki" => 595u16,
        "application/vnd.anser-web-certificate-issue-initiation" => 596u16,
        "application/vnd.antix.game-component" => 597u16,
        "application/vnd.apache.arrow.file" => 598u16,
        "application/vnd.apache.arrow.stream" => 599u16,
        "application/vnd.apache.parquet" => 600u16,
        "application/vnd.apache.thrift.binary" => 601u16,
        "application/vnd.apache.thrift.compact" => 602u16,
        "application/vnd.apache.thrift.json" => 603u16,
        "application/vnd.apexlang" => 604u16,
        "application/vnd.api+json" => 605u16,
        "application/vnd.aplextor.warrp+json" => 606u16,
        "application/vnd.apothekende.reservation+json" => 607u16,
        "application/vnd.apple.installer+xml" => 608u16,
        "application/vnd.apple.keynote" => 609u16,
        "application/vnd.apple.mpegurl" => 610u16,
        "application/vnd.apple.numbers" => 611u16,
        "application/vnd.apple.pages" => 612u16,
        "application/vnd.arastra.swi" => 613u16,
        "application/vnd.aristanetworks.swi" => 614u16,
        "application/vnd.artisan+json" => 615u16,
        "application/vnd.artsquare" => 616u16,
        "application/vnd.astraea-software.iota" => 617u16,
        "application/vnd.audiograph" => 618u16,
        "application/vnd.autopackage" => 619u16,
        "application/vnd.avalon+json" => 620u16,
        "application/vnd.avistar+xml" => 621u16,
        "application/vnd.balsamiq.bmml+xml" => 622u16,
        "application/vnd.balsamiq.bmpr" => 623u16,
        "application/vnd.banana-accounting" => 624u16,
        "application/vnd.bbf.usp.error" => 625u16,
        "application/vnd.bbf.usp.msg" => 626u16,
        "application/vnd.bbf.usp.msg+json" => 627u16,
        "application/vnd.bekitzur-stech+json" => 628u16,
        "application/vnd.belightsoft.lhzd+zip" => 629u16,
        "application/vnd.belightsoft.lhzl+zip" => 630u16,
        "application/vnd.bint.med-content" => 631u16,
        "application/vnd.biopax.rdf+xml" => 632u16,
        "application/vnd.blink-idb-value-wrapper" => 633u16,
        "application/vnd.blueice.multipass" => 634u16,
        "application/vnd.bluetooth.ep.oob" => 635u16,
        "application/vnd.bluetooth.le.oob" => 636u16,
        "application/vnd.bmi" => 637u16,
        "application/vnd.bpf" => 638u16,
        "application/vnd.bpf3" => 639u16,
        "application/vnd.businessobjects" => 640u16,
        "application/vnd.byu.uapi+json" => 641u16,
        "application/vnd.bzip3" => 642u16,
        "application/vnd.cab-jscript" => 643u16,
        "application/vnd.canon-cpdl" => 644u16,
        "application/vnd.canon-lips" => 645u16,
        "application/vnd.capasystems-pg+json" => 646u16,
        "application/vnd.cendio.thinlinc.clientconf" => 647u16,
        "application/vnd.century-systems.tcp_stream" => 648u16,
        "application/vnd.chemdraw+xml" => 649u16,
        "application/vnd.chess-pgn" => 650u16,
        "application/vnd.chipnuts.karaoke-mmd" => 651u16,
        "application/vnd.ciedi" => 652u16,
        "application/vnd.cinderella" => 653u16,
        "application/vnd.cirpack.isdn-ext" => 654u16,
        "application/vnd.citationstyles.style+xml" => 655u16,
        "application/vnd.claymore" => 656u16,
        "application/vnd.cloanto.rp9" => 657u16,
        "application/vnd.clonk.c4group" => 658u16,
        "application/vnd.cluetrust.cartomobile-config" => 659u16,
        "application/vnd.cluetrust.cartomobile-config-pkg" => 660u16,
        "application/vnd.cncf.helm.chart.content.v1.tar+gzip" => 661u16,
        "application/vnd.cncf.helm.chart.provenance.v1.prov" => 662u16,
        "application/vnd.cncf.helm.config.v1+json" => 663u16,
        "application/vnd.coffeescript" => 664u16,
        "application/vnd.collabio.xodocuments.document" => 665u16,
        "application/vnd.collabio.xodocuments.document-template" => 666u16,
        "application/vnd.collabio.xodocuments.presentation" => 667u16,
        "application/vnd.collabio.xodocuments.presentation-template" => 668u16,
        "application/vnd.collabio.xodocuments.spreadsheet" => 669u16,
        "application/vnd.collabio.xodocuments.spreadsheet-template" => 670u16,
        "application/vnd.collection+json" => 671u16,
        "application/vnd.collection.doc+json" => 672u16,
        "application/vnd.collection.next+json" => 673u16,
        "application/vnd.comicbook+zip" => 674u16,
        "application/vnd.comicbook-rar" => 675u16,
        "application/vnd.commerce-battelle" => 676u16,
        "application/vnd.commonspace" => 677u16,
        "application/vnd.contact.cmsg" => 678u16,
        "application/vnd.coreos.ignition+json" => 679u16,
        "application/vnd.cosmocaller" => 680u16,
        "application/vnd.crick.clicker" => 681u16,
        "application/vnd.crick.clicker.keyboard" => 682u16,
        "application/vnd.crick.clicker.palette" => 683u16,
        "application/vnd.crick.clicker.template" => 684u16,
        "application/vnd.crick.clicker.wordbank" => 685u16,
        "application/vnd.criticaltools.wbs+xml" => 686u16,
        "application/vnd.cryptii.pipe+json" => 687u16,
        "application/vnd.crypto-shade-file" => 688u16,
        "application/vnd.cryptomator.encrypted" => 689u16,
        "application/vnd.cryptomator.vault" => 690u16,
        "application/vnd.ctc-posml" => 691u16,
        "application/vnd.ctct.ws+xml" => 692u16,
        "application/vnd.cups-pdf" => 693u16,
        "application/vnd.cups-postscript" => 694u16,
        "application/vnd.cups-ppd" => 695u16,
        "application/vnd.cups-raster" => 696u16,
        "application/vnd.cups-raw" => 697u16,
        "application/vnd.curl" => 698u16,
        "application/vnd.cyan.dean.root+xml" => 699u16,
        "application/vnd.cybank" => 700u16,
        "application/vnd.cyclonedx+json" => 701u16,
        "application/vnd.cyclonedx+xml" => 702u16,
        "application/vnd.d2l.coursepackage1p0+zip" => 703u16,
        "application/vnd.d3m-dataset" => 704u16,
        "application/vnd.d3m-problem" => 705u16,
        "application/vnd.dart" => 706u16,
        "application/vnd.data-vision.rdz" => 707u16,
        "application/vnd.datalog" => 708u16,
        "application/vnd.datapackage+json" => 709u16,
        "application/vnd.dataresource+json" => 710u16,
        "application/vnd.dbf" => 711u16,
        "application/vnd.debian.binary-package" => 712u16,
        "application/vnd.dece.data" => 713u16,
        "application/vnd.dece.ttml+xml" => 714u16,
        "application/vnd.dece.unspecified" => 715u16,
        "application/vnd.dece.zip" => 716u16,
        "application/vnd.denovo.fcselayout-link" => 717u16,
        "application/vnd.desmume.movie" => 718u16,
        "application/vnd.dir-bi.plate-dl-nosuffix" => 719u16,
        "application/vnd.dm.delegation+xml" => 720u16,
        "application/vnd.dna" => 721u16,
        "application/vnd.document+json" => 722u16,
        "application/vnd.dolby.mobile.1" => 723u16,
        "application/vnd.dolby.mobile.2" => 724u16,
        "application/vnd.doremir.scorecloud-binary-document" => 725u16,
        "application/vnd.dpgraph" => 726u16,
        "application/vnd.dreamfactory" => 727u16,
        "application/vnd.drive+json" => 728u16,
        "application/vnd.dtg.local" => 729u16,
        "application/vnd.dtg.local.flash" => 730u16,
        "application/vnd.dtg.local.html" => 731u16,
        "application/vnd.dvb.ait" => 732u16,
        "application/vnd.dvb.dvbisl+xml" => 733u16,
        "application/vnd.dvb.dvbj" => 734u16,
        "application/vnd.dvb.esgcontainer" => 735u16,
        "application/vnd.dvb.ipdcdftnotifaccess" => 736u16,
        "application/vnd.dvb.ipdcesgaccess" => 737u16,
        "application/vnd.dvb.ipdcesgaccess2" => 738u16,
        "application/vnd.dvb.ipdcesgpdd" => 739u16,
        "application/vnd.dvb.ipdcroaming" => 740u16,
        "application/vnd.dvb.iptv.alfec-base" => 741u16,
        "application/vnd.dvb.iptv.alfec-enhancement" => 742u16,
        "application/vnd.dvb.notif-aggregate-root+xml" => 743u16,
        "application/vnd.dvb.notif-container+xml" => 744u16,
        "application/vnd.dvb.notif-generic+xml" => 745u16,
        "application/vnd.dvb.notif-ia-msglist+xml" => 746u16,
        "application/vnd.dvb.notif-ia-registration-request+xml" => 747u16,
        "application/vnd.dvb.notif-ia-registration-response+xml" => 748u16,
        "application/vnd.dvb.notif-init+xml" => 749u16,
        "application/vnd.dvb.pfr" => 750u16,
        "application/vnd.dvb.service" => 751u16,
        "application/vnd.dxr" => 752u16,
        "application/vnd.dynageo" => 753u16,
        "application/vnd.dzr" => 754u16,
        "application/vnd.easykaraoke.cdgdownload" => 755u16,
        "application/vnd.ecdis-update" => 756u16,
        "application/vnd.ecip.rlp" => 757u16,
        "application/vnd.eclipse.ditto+json" => 758u16,
        "application/vnd.ecowin.chart" => 759u16,
        "application/vnd.ecowin.filerequest" => 760u16,
        "application/vnd.ecowin.fileupdate" => 761u16,
        "application/vnd.ecowin.series" => 762u16,
        "application/vnd.ecowin.seriesrequest" => 763u16,
        "application/vnd.ecowin.seriesupdate" => 764u16,
        "application/vnd.efi.img" => 765u16,
        "application/vnd.efi.iso" => 766u16,
        "application/vnd.eln+zip" => 767u16,
        "application/vnd.emclient.accessrequest+xml" => 768u16,
        "application/vnd.enliven" => 769u16,
        "application/vnd.enphase.envoy" => 770u16,
        "application/vnd.eprints.data+xml" => 771u16,
        "application/vnd.epson.esf" => 772u16,
        "application/vnd.epson.msf" => 773u16,
        "application/vnd.epson.quickanime" => 774u16,
        "application/vnd.epson.salt" => 775u16,
        "application/vnd.epson.ssf" => 776u16,
        "application/vnd.ericsson.quickcall" => 777u16,
        "application/vnd.erofs" => 778u16,
        "application/vnd.espass-espass+zip" => 779u16,
        "application/vnd.eszigno3+xml" => 780u16,
        "application/vnd.etsi.aoc+xml" => 781u16,
        "application/vnd.etsi.asic-e+zip" => 782u16,
        "application/vnd.etsi.asic-s+zip" => 783u16,
        "application/vnd.etsi.cug+xml" => 784u16,
        "application/vnd.etsi.iptvcommand+xml" => 785u16,
        "application/vnd.etsi.iptvdiscovery+xml" => 786u16,
        "application/vnd.etsi.iptvprofile+xml" => 787u16,
        "application/vnd.etsi.iptvsad-bc+xml" => 788u16,
        "application/vnd.etsi.iptvsad-cod+xml" => 789u16,
        "application/vnd.etsi.iptvsad-npvr+xml" => 790u16,
        "application/vnd.etsi.iptvservice+xml" => 791u16,
        "application/vnd.etsi.iptvsync+xml" => 792u16,
        "application/vnd.etsi.iptvueprofile+xml" => 793u16,
        "application/vnd.etsi.mcid+xml" => 794u16,
        "application/vnd.etsi.mheg5" => 795u16,
        "application/vnd.etsi.overload-control-policy-dataset+xml" => 796u16,
        "application/vnd.etsi.pstn+xml" => 797u16,
        "application/vnd.etsi.sci+xml" => 798u16,
        "application/vnd.etsi.simservs+xml" => 799u16,
        "application/vnd.etsi.timestamp-token" => 800u16,
        "application/vnd.etsi.tsl+xml" => 801u16,
        "application/vnd.etsi.tsl.der" => 802u16,
        "application/vnd.eu.kasparian.car+json" => 803u16,
        "application/vnd.eudora.data" => 804u16,
        "application/vnd.evolv.ecig.profile" => 805u16,
        "application/vnd.evolv.ecig.settings" => 806u16,
        "application/vnd.evolv.ecig.theme" => 807u16,
        "application/vnd.exstream-empower+zip" => 808u16,
        "application/vnd.exstream-package" => 809u16,
        "application/vnd.ezpix-album" => 810u16,
        "application/vnd.ezpix-package" => 811u16,
        "application/vnd.f-secure.mobile" => 812u16,
        "application/vnd.familysearch.gedcom+zip" => 813u16,
        "application/vnd.fastcopy-disk-image" => 814u16,
        "application/vnd.fdsn.mseed" => 815u16,
        "application/vnd.fdsn.seed" => 816u16,
        "application/vnd.ffsns" => 817u16,
        "application/vnd.ficlab.flb+zip" => 818u16,
        "application/vnd.filmit.zfc" => 819u16,
        "application/vnd.fints" => 820u16,
        "application/vnd.firemonkeys.cloudcell" => 821u16,
        "application/vnd.fluxtime.clip" => 822u16,
        "application/vnd.font-fontforge-sfd" => 823u16,
        "application/vnd.framemaker" => 824u16,
        "application/vnd.freelog.comic" => 825u16,
        "application/vnd.frogans.fnc" => 826u16,
        "application/vnd.frogans.ltf" => 827u16,
        "application/vnd.fsc.weblaunch" => 828u16,
        "application/vnd.fujifilm.fb.docuworks" => 829u16,
        "application/vnd.fujifilm.fb.docuworks.binder" => 830u16,
        "application/vnd.fujifilm.fb.docuworks.container" => 831u16,
        "application/vnd.fujifilm.fb.jfi+xml" => 832u16,
        "application/vnd.fujitsu.oasys" => 833u16,
        "application/vnd.fujitsu.oasys2" => 834u16,
        "application/vnd.fujitsu.oasys3" => 835u16,
        "application/vnd.fujitsu.oasysgp" => 836u16,
        "application/vnd.fujitsu.oasysprs" => 837u16,
        "application/vnd.fujixerox.ART-EX" => 838u16,
        "application/vnd.fujixerox.ART4" => 839u16,
        "application/vnd.fujixerox.HBPL" => 840u16,
        "application/vnd.fujixerox.ddd" => 841u16,
        "application/vnd.fujixerox.docuworks" => 842u16,
        "application/vnd.fujixerox.docuworks.binder" => 843u16,
        "application/vnd.fujixerox.docuworks.container" => 844u16,
        "application/vnd.fut-misnet" => 845u16,
        "application/vnd.futoin+cbor" => 846u16,
        "application/vnd.futoin+json" => 847u16,
        "application/vnd.fuzzysheet" => 848u16,
        "application/vnd.genomatix.tuxedo" => 849u16,
        "application/vnd.genozip" => 850u16,
        "application/vnd.gentics.grd+json" => 851u16,
        "application/vnd.gentoo.catmetadata+xml" => 852u16,
        "application/vnd.gentoo.ebuild" => 853u16,
        "application/vnd.gentoo.eclass" => 854u16,
        "application/vnd.gentoo.gpkg" => 855u16,
        "application/vnd.gentoo.manifest" => 856u16,
        "application/vnd.gentoo.pkgmetadata+xml" => 857u16,
        "application/vnd.gentoo.xpak" => 858u16,
        "application/vnd.geo+json" => 859u16,
        "application/vnd.geocube+xml" => 860u16,
        "application/vnd.geogebra.file" => 861u16,
        "application/vnd.geogebra.slides" => 862u16,
        "application/vnd.geogebra.tool" => 863u16,
        "application/vnd.geometry-explorer" => 864u16,
        "application/vnd.geonext" => 865u16,
        "application/vnd.geoplan" => 866u16,
        "application/vnd.geospace" => 867u16,
        "application/vnd.gerber" => 868u16,
        "application/vnd.globalplatform.card-content-mgt" => 869u16,
        "application/vnd.globalplatform.card-content-mgt-response" => 870u16,
        "application/vnd.gmx" => 871u16,
        "application/vnd.gnu.taler.exchange+json" => 872u16,
        "application/vnd.gnu.taler.merchant+json" => 873u16,
        "application/vnd.google-earth.kml+xml" => 874u16,
        "application/vnd.google-earth.kmz" => 875u16,
        "application/vnd.gov.sk.e-form+xml" => 876u16,
        "application/vnd.gov.sk.e-form+zip" => 877u16,
        "application/vnd.gov.sk.xmldatacontainer+xml" => 878u16,
        "application/vnd.gpxsee.map+xml" => 879u16,
        "application/vnd.grafeq" => 880u16,
        "application/vnd.gridmp" => 881u16,
        "application/vnd.groove-account" => 882u16,
        "application/vnd.groove-help" => 883u16,
        "application/vnd.groove-identity-message" => 884u16,
        "application/vnd.groove-injector" => 885u16,
        "application/vnd.groove-tool-message" => 886u16,
        "application/vnd.groove-tool-template" => 887u16,
        "application/vnd.groove-vcard" => 888u16,
        "application/vnd.hal+json" => 889u16,
        "application/vnd.hal+xml" => 890u16,
        "application/vnd.hbci" => 891u16,
        "application/vnd.hc+json" => 892u16,
        "application/vnd.hcl-bireports" => 893u16,
        "application/vnd.hdt" => 894u16,
        "application/vnd.heroku+json" => 895u16,
        "application/vnd.hhe.lesson-player" => 896u16,
        "application/vnd.hp-HPGL" => 897u16,
        "application/vnd.hp-PCL" => 898u16,
        "application/vnd.hp-PCLXL" => 899u16,
        "application/vnd.hp-hpid" => 900u16,
        "application/vnd.hp-hps" => 901u16,
        "application/vnd.hp-jlyt" => 902u16,
        "application/vnd.hsl" => 903u16,
        "application/vnd.httphone" => 904u16,
        "application/vnd.hydrostatix.sof-data" => 905u16,
        "application/vnd.hyper+json" => 906u16,
        "application/vnd.hyper-item+json" => 907u16,
        "application/vnd.hyperdrive+json" => 908u16,
        "application/vnd.hzn-3d-crossword" => 909u16,
        "application/vnd.ibm.MiniPay" => 910u16,
        "application/vnd.ibm.afplinedata" => 911u16,
        "application/vnd.ibm.electronic-media" => 912u16,
        "application/vnd.ibm.modcap" => 913u16,
        "application/vnd.ibm.rights-management" => 914u16,
        "application/vnd.ibm.secure-container" => 915u16,
        "application/vnd.iccprofile" => 916u16,
        "application/vnd.ieee.1905" => 917u16,
        "application/vnd.igloader" => 918u16,
        "application/vnd.imagemeter.folder+zip" => 919u16,
        "application/vnd.imagemeter.image+zip" => 920u16,
        "application/vnd.immervision-ivp" => 921u16,
        "application/vnd.immervision-ivu" => 922u16,
        "application/vnd.ims.imsccv1p1" => 923u16,
        "application/vnd.ims.imsccv1p2" => 924u16,
        "application/vnd.ims.imsccv1p3" => 925u16,
        "application/vnd.ims.lis.v2.result+json" => 926u16,
        "application/vnd.ims.lti.v2.toolconsumerprofile+json" => 927u16,
        "application/vnd.ims.lti.v2.toolproxy+json" => 928u16,
        "application/vnd.ims.lti.v2.toolproxy.id+json" => 929u16,
        "application/vnd.ims.lti.v2.toolsettings+json" => 930u16,
        "application/vnd.ims.lti.v2.toolsettings.simple+json" => 931u16,
        "application/vnd.informedcontrol.rms+xml" => 932u16,
        "application/vnd.informix-visionary" => 933u16,
        "application/vnd.infotech.project" => 934u16,
        "application/vnd.infotech.project+xml" => 935u16,
        "application/vnd.innopath.wamp.notification" => 936u16,
        "application/vnd.insors.igm" => 937u16,
        "application/vnd.intercon.formnet" => 938u16,
        "application/vnd.intergeo" => 939u16,
        "application/vnd.intertrust.digibox" => 940u16,
        "application/vnd.intertrust.nncp" => 941u16,
        "application/vnd.intu.qbo" => 942u16,
        "application/vnd.intu.qfx" => 943u16,
        "application/vnd.ipfs.ipns-record" => 944u16,
        "application/vnd.ipld.car" => 945u16,
        "application/vnd.ipld.dag-cbor" => 946u16,
        "application/vnd.ipld.dag-json" => 947u16,
        "application/vnd.ipld.raw" => 948u16,
        "application/vnd.iptc.g2.catalogitem+xml" => 949u16,
        "application/vnd.iptc.g2.conceptitem+xml" => 950u16,
        "application/vnd.iptc.g2.knowledgeitem+xml" => 951u16,
        "application/vnd.iptc.g2.newsitem+xml" => 952u16,
        "application/vnd.iptc.g2.newsmessage+xml" => 953u16,
        "application/vnd.iptc.g2.packageitem+xml" => 954u16,
        "application/vnd.iptc.g2.planningitem+xml" => 955u16,
        "application/vnd.ipunplugged.rcprofile" => 956u16,
        "application/vnd.irepository.package+xml" => 957u16,
        "application/vnd.is-xpr" => 958u16,
        "application/vnd.isac.fcs" => 959u16,
        "application/vnd.iso11783-10+zip" => 960u16,
        "application/vnd.jam" => 961u16,
        "application/vnd.japannet-directory-service" => 962u16,
        "application/vnd.japannet-jpnstore-wakeup" => 963u16,
        "application/vnd.japannet-payment-wakeup" => 964u16,
        "application/vnd.japannet-registration" => 965u16,
        "application/vnd.japannet-registration-wakeup" => 966u16,
        "application/vnd.japannet-setstore-wakeup" => 967u16,
        "application/vnd.japannet-verification" => 968u16,
        "application/vnd.japannet-verification-wakeup" => 969u16,
        "application/vnd.jcp.javame.midlet-rms" => 970u16,
        "application/vnd.jisp" => 971u16,
        "application/vnd.joost.joda-archive" => 972u16,
        "application/vnd.jsk.isdn-ngn" => 973u16,
        "application/vnd.kahootz" => 974u16,
        "application/vnd.kde.karbon" => 975u16,
        "application/vnd.kde.kchart" => 976u16,
        "application/vnd.kde.kformula" => 977u16,
        "application/vnd.kde.kivio" => 978u16,
        "application/vnd.kde.kontour" => 979u16,
        "application/vnd.kde.kpresenter" => 980u16,
        "application/vnd.kde.kspread" => 981u16,
        "application/vnd.kde.kword" => 982u16,
        "application/vnd.kenameaapp" => 983u16,
        "application/vnd.kidspiration" => 984u16,
        "application/vnd.koan" => 985u16,
        "application/vnd.kodak-descriptor" => 986u16,
        "application/vnd.las" => 987u16,
        "application/vnd.las.las+json" => 988u16,
        "application/vnd.las.las+xml" => 989u16,
        "application/vnd.laszip" => 990u16,
        "application/vnd.ldev.productlicensing" => 991u16,
        "application/vnd.leap+json" => 992u16,
        "application/vnd.liberty-request+xml" => 993u16,
        "application/vnd.llamagraphics.life-balance.desktop" => 994u16,
        "application/vnd.llamagraphics.life-balance.exchange+xml" => 995u16,
        "application/vnd.logipipe.circuit+zip" => 996u16,
        "application/vnd.loom" => 997u16,
        "application/vnd.lotus-1-2-3" => 998u16,
        "application/vnd.lotus-approach" => 999u16,
        "application/vnd.lotus-freelance" => 1000u16,
        "application/vnd.lotus-notes" => 1001u16,
        "application/vnd.lotus-organizer" => 1002u16,
        "application/vnd.lotus-screencam" => 1003u16,
        "application/vnd.lotus-wordpro" => 1004u16,
        "application/vnd.macports.portpkg" => 1005u16,
        "application/vnd.mapbox-vector-tile" => 1006u16,
        "application/vnd.marlin.drm.actiontoken+xml" => 1007u16,
        "application/vnd.marlin.drm.conftoken+xml" => 1008u16,
        "application/vnd.marlin.drm.license+xml" => 1009u16,
        "application/vnd.marlin.drm.mdcf" => 1010u16,
        "application/vnd.mason+json" => 1011u16,
        "application/vnd.maxar.archive.3tz+zip" => 1012u16,
        "application/vnd.maxmind.maxmind-db" => 1013u16,
        "application/vnd.mcd" => 1014u16,
        "application/vnd.mdl" => 1015u16,
        "application/vnd.mdl-mbsdf" => 1016u16,
        "application/vnd.medcalcdata" => 1017u16,
        "application/vnd.mediastation.cdkey" => 1018u16,
        "application/vnd.medicalholodeck.recordxr" => 1019u16,
        "application/vnd.meridian-slingshot" => 1020u16,
        "application/vnd.mermaid" => 1021u16,
        "application/vnd.mfmp" => 1022u16,
        "application/vnd.micro+json" => 1023u16,
        "application/vnd.micrografx.flo" => 1024u16,
        "application/vnd.micrografx.igx" => 1025u16,
        "application/vnd.microsoft.portable-executable" => 1026u16,
        "application/vnd.microsoft.windows.thumbnail-cache" => 1027u16,
        "application/vnd.miele+json" => 1028u16,
        "application/vnd.mif" => 1029u16,
        "application/vnd.minisoft-hp3000-save" => 1030u16,
        "application/vnd.mitsubishi.misty-guard.trustweb" => 1031u16,
        "application/vnd.modl" => 1032u16,
        "application/vnd.mophun.application" => 1033u16,
        "application/vnd.mophun.certificate" => 1034u16,
        "application/vnd.motorola.flexsuite" => 1035u16,
        "application/vnd.motorola.flexsuite.adsi" => 1036u16,
        "application/vnd.motorola.flexsuite.fis" => 1037u16,
        "application/vnd.motorola.flexsuite.gotap" => 1038u16,
        "application/vnd.motorola.flexsuite.kmr" => 1039u16,
        "application/vnd.motorola.flexsuite.ttc" => 1040u16,
        "application/vnd.motorola.flexsuite.wem" => 1041u16,
        "application/vnd.motorola.iprm" => 1042u16,
        "application/vnd.mozilla.xul+xml" => 1043u16,
        "application/vnd.ms-3mfdocument" => 1044u16,
        "application/vnd.ms-PrintDeviceCapabilities+xml" => 1045u16,
        "application/vnd.ms-PrintSchemaTicket+xml" => 1046u16,
        "application/vnd.ms-artgalry" => 1047u16,
        "application/vnd.ms-asf" => 1048u16,
        "application/vnd.ms-cab-compressed" => 1049u16,
        "application/vnd.ms-excel" => 1050u16,
        "application/vnd.ms-excel.addin.macroEnabled.12" => 1051u16,
        "application/vnd.ms-excel.sheet.binary.macroEnabled.12" => 1052u16,
        "application/vnd.ms-excel.sheet.macroEnabled.12" => 1053u16,
        "application/vnd.ms-excel.template.macroEnabled.12" => 1054u16,
        "application/vnd.ms-fontobject" => 1055u16,
        "application/vnd.ms-htmlhelp" => 1056u16,
        "application/vnd.ms-ims" => 1057u16,
        "application/vnd.ms-lrm" => 1058u16,
        "application/vnd.ms-office.activeX+xml" => 1059u16,
        "application/vnd.ms-officetheme" => 1060u16,
        "application/vnd.ms-playready.initiator+xml" => 1061u16,
        "application/vnd.ms-powerpoint" => 1062u16,
        "application/vnd.ms-powerpoint.addin.macroEnabled.12" => 1063u16,
        "application/vnd.ms-powerpoint.presentation.macroEnabled.12" => 1064u16,
        "application/vnd.ms-powerpoint.slide.macroEnabled.12" => 1065u16,
        "application/vnd.ms-powerpoint.slideshow.macroEnabled.12" => 1066u16,
        "application/vnd.ms-powerpoint.template.macroEnabled.12" => 1067u16,
        "application/vnd.ms-project" => 1068u16,
        "application/vnd.ms-tnef" => 1069u16,
        "application/vnd.ms-windows.devicepairing" => 1070u16,
        "application/vnd.ms-windows.nwprinting.oob" => 1071u16,
        "application/vnd.ms-windows.printerpairing" => 1072u16,
        "application/vnd.ms-windows.wsd.oob" => 1073u16,
        "application/vnd.ms-wmdrm.lic-chlg-req" => 1074u16,
        "application/vnd.ms-wmdrm.lic-resp" => 1075u16,
        "application/vnd.ms-wmdrm.meter-chlg-req" => 1076u16,
        "application/vnd.ms-wmdrm.meter-resp" => 1077u16,
        "application/vnd.ms-word.document.macroEnabled.12" => 1078u16,
        "application/vnd.ms-word.template.macroEnabled.12" => 1079u16,
        "application/vnd.ms-works" => 1080u16,
        "application/vnd.ms-wpl" => 1081u16,
        "application/vnd.ms-xpsdocument" => 1082u16,
        "application/vnd.msa-disk-image" => 1083u16,
        "application/vnd.mseq" => 1084u16,
        "application/vnd.msign" => 1085u16,
        "application/vnd.multiad.creator" => 1086u16,
        "application/vnd.multiad.creator.cif" => 1087u16,
        "application/vnd.music-niff" => 1088u16,
        "application/vnd.musician" => 1089u16,
        "application/vnd.muvee.style" => 1090u16,
        "application/vnd.mynfc" => 1091u16,
        "application/vnd.nacamar.ybrid+json" => 1092u16,
        "application/vnd.nato.bindingdataobject+cbor" => 1093u16,
        "application/vnd.nato.bindingdataobject+json" => 1094u16,
        "application/vnd.nato.bindingdataobject+xml" => 1095u16,
        "application/vnd.nato.openxmlformats-package.iepd+zip" => 1096u16,
        "application/vnd.ncd.control" => 1097u16,
        "application/vnd.ncd.reference" => 1098u16,
        "application/vnd.nearst.inv+json" => 1099u16,
        "application/vnd.nebumind.line" => 1100u16,
        "application/vnd.nervana" => 1101u16,
        "application/vnd.netfpx" => 1102u16,
        "application/vnd.neurolanguage.nlu" => 1103u16,
        "application/vnd.nimn" => 1104u16,
        "application/vnd.nintendo.nitro.rom" => 1105u16,
        "application/vnd.nintendo.snes.rom" => 1106u16,
        "application/vnd.nitf" => 1107u16,
        "application/vnd.noblenet-directory" => 1108u16,
        "application/vnd.noblenet-sealer" => 1109u16,
        "application/vnd.noblenet-web" => 1110u16,
        "application/vnd.nokia.catalogs" => 1111u16,
        "application/vnd.nokia.conml+wbxml" => 1112u16,
        "application/vnd.nokia.conml+xml" => 1113u16,
        "application/vnd.nokia.iSDS-radio-presets" => 1114u16,
        "application/vnd.nokia.iptv.config+xml" => 1115u16,
        "application/vnd.nokia.landmark+wbxml" => 1116u16,
        "application/vnd.nokia.landmark+xml" => 1117u16,
        "application/vnd.nokia.landmarkcollection+xml" => 1118u16,
        "application/vnd.nokia.n-gage.ac+xml" => 1119u16,
        "application/vnd.nokia.n-gage.data" => 1120u16,
        "application/vnd.nokia.n-gage.symbian.install" => 1121u16,
        "application/vnd.nokia.ncd" => 1122u16,
        "application/vnd.nokia.pcd+wbxml" => 1123u16,
        "application/vnd.nokia.pcd+xml" => 1124u16,
        "application/vnd.nokia.radio-preset" => 1125u16,
        "application/vnd.nokia.radio-presets" => 1126u16,
        "application/vnd.novadigm.EDM" => 1127u16,
        "application/vnd.novadigm.EDX" => 1128u16,
        "application/vnd.novadigm.EXT" => 1129u16,
        "application/vnd.ntt-local.content-share" => 1130u16,
        "application/vnd.ntt-local.file-transfer" => 1131u16,
        "application/vnd.ntt-local.ogw_remote-access" => 1132u16,
        "application/vnd.ntt-local.sip-ta_remote" => 1133u16,
        "application/vnd.ntt-local.sip-ta_tcp_stream" => 1134u16,
        "application/vnd.oai.workflows" => 1135u16,
        "application/vnd.oai.workflows+json" => 1136u16,
        "application/vnd.oai.workflows+yaml" => 1137u16,
        "application/vnd.oasis.opendocument.base" => 1138u16,
        "application/vnd.oasis.opendocument.chart" => 1139u16,
        "application/vnd.oasis.opendocument.chart-template" => 1140u16,
        "application/vnd.oasis.opendocument.database" => 1141u16,
        "application/vnd.oasis.opendocument.formula" => 1142u16,
        "application/vnd.oasis.opendocument.formula-template" => 1143u16,
        "application/vnd.oasis.opendocument.graphics" => 1144u16,
        "application/vnd.oasis.opendocument.graphics-template" => 1145u16,
        "application/vnd.oasis.opendocument.image" => 1146u16,
        "application/vnd.oasis.opendocument.image-template" => 1147u16,
        "application/vnd.oasis.opendocument.presentation" => 1148u16,
        "application/vnd.oasis.opendocument.presentation-template" => 1149u16,
        "application/vnd.oasis.opendocument.spreadsheet" => 1150u16,
        "application/vnd.oasis.opendocument.spreadsheet-template" => 1151u16,
        "application/vnd.oasis.opendocument.text" => 1152u16,
        "application/vnd.oasis.opendocument.text-master" => 1153u16,
        "application/vnd.oasis.opendocument.text-master-template" => 1154u16,
        "application/vnd.oasis.opendocument.text-template" => 1155u16,
        "application/vnd.oasis.opendocument.text-web" => 1156u16,
        "application/vnd.obn" => 1157u16,
        "application/vnd.ocf+cbor" => 1158u16,
        "application/vnd.oci.image.manifest.v1+json" => 1159u16,
        "application/vnd.oftn.l10n+json" => 1160u16,
        "application/vnd.oipf.contentaccessdownload+xml" => 1161u16,
        "application/vnd.oipf.contentaccessstreaming+xml" => 1162u16,
        "application/vnd.oipf.cspg-hexbinary" => 1163u16,
        "application/vnd.oipf.dae.svg+xml" => 1164u16,
        "application/vnd.oipf.dae.xhtml+xml" => 1165u16,
        "application/vnd.oipf.mippvcontrolmessage+xml" => 1166u16,
        "application/vnd.oipf.pae.gem" => 1167u16,
        "application/vnd.oipf.spdiscovery+xml" => 1168u16,
        "application/vnd.oipf.spdlist+xml" => 1169u16,
        "application/vnd.oipf.ueprofile+xml" => 1170u16,
        "application/vnd.oipf.userprofile+xml" => 1171u16,
        "application/vnd.olpc-sugar" => 1172u16,
        "application/vnd.oma-scws-config" => 1173u16,
        "application/vnd.oma-scws-http-request" => 1174u16,
        "application/vnd.oma-scws-http-response" => 1175u16,
        "application/vnd.oma.bcast.associated-procedure-parameter+xml" => 1176u16,
        "application/vnd.oma.bcast.drm-trigger+xml" => 1177u16,
        "application/vnd.oma.bcast.imd+xml" => 1178u16,
        "application/vnd.oma.bcast.ltkm" => 1179u16,
        "application/vnd.oma.bcast.notification+xml" => 1180u16,
        "application/vnd.oma.bcast.provisioningtrigger" => 1181u16,
        "application/vnd.oma.bcast.sgboot" => 1182u16,
        "application/vnd.oma.bcast.sgdd+xml" => 1183u16,
        "application/vnd.oma.bcast.sgdu" => 1184u16,
        "application/vnd.oma.bcast.simple-symbol-container" => 1185u16,
        "application/vnd.oma.bcast.smartcard-trigger+xml" => 1186u16,
        "application/vnd.oma.bcast.sprov+xml" => 1187u16,
        "application/vnd.oma.bcast.stkm" => 1188u16,
        "application/vnd.oma.cab-address-book+xml" => 1189u16,
        "application/vnd.oma.cab-feature-handler+xml" => 1190u16,
        "application/vnd.oma.cab-pcc+xml" => 1191u16,
        "application/vnd.oma.cab-subs-invite+xml" => 1192u16,
        "application/vnd.oma.cab-user-prefs+xml" => 1193u16,
        "application/vnd.oma.dcd" => 1194u16,
        "application/vnd.oma.dcdc" => 1195u16,
        "application/vnd.oma.dd2+xml" => 1196u16,
        "application/vnd.oma.drm.risd+xml" => 1197u16,
        "application/vnd.oma.group-usage-list+xml" => 1198u16,
        "application/vnd.oma.lwm2m+cbor" => 1199u16,
        "application/vnd.oma.lwm2m+json" => 1200u16,
        "application/vnd.oma.lwm2m+tlv" => 1201u16,
        "application/vnd.oma.pal+xml" => 1202u16,
        "application/vnd.oma.poc.detailed-progress-report+xml" => 1203u16,
        "application/vnd.oma.poc.final-report+xml" => 1204u16,
        "application/vnd.oma.poc.groups+xml" => 1205u16,
        "application/vnd.oma.poc.invocation-descriptor+xml" => 1206u16,
        "application/vnd.oma.poc.optimized-progress-report+xml" => 1207u16,
        "application/vnd.oma.push" => 1208u16,
        "application/vnd.oma.scidm.messages+xml" => 1209u16,
        "application/vnd.oma.xcap-directory+xml" => 1210u16,
        "application/vnd.omads-email+xml" => 1211u16,
        "application/vnd.omads-file+xml" => 1212u16,
        "application/vnd.omads-folder+xml" => 1213u16,
        "application/vnd.omaloc-supl-init" => 1214u16,
        "application/vnd.onepager" => 1215u16,
        "application/vnd.onepagertamp" => 1216u16,
        "application/vnd.onepagertamx" => 1217u16,
        "application/vnd.onepagertat" => 1218u16,
        "application/vnd.onepagertatp" => 1219u16,
        "application/vnd.onepagertatx" => 1220u16,
        "application/vnd.onvif.metadata" => 1221u16,
        "application/vnd.openblox.game+xml" => 1222u16,
        "application/vnd.openblox.game-binary" => 1223u16,
        "application/vnd.openeye.oeb" => 1224u16,
        "application/vnd.openstreetmap.data+xml" => 1225u16,
        "application/vnd.opentimestamps.ots" => 1226u16,
        "application/vnd.openxmlformats-officedocument.custom-properties+xml" => 1227u16,
        "application/vnd.openxmlformats-officedocument.customXmlProperties+xml" => 1228u16,
        "application/vnd.openxmlformats-officedocument.drawing+xml" => 1229u16,
        "application/vnd.openxmlformats-officedocument.drawingml.chart+xml" => 1230u16,
        "application/vnd.openxmlformats-officedocument.drawingml.chartshapes+xml" => 1231u16,
        "application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml" => 1232u16,
        "application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml" => 1233u16,
        "application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml" => 1234u16,
        "application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml" => 1235u16,
        "application/vnd.openxmlformats-officedocument.extended-properties+xml" => 1236u16,
        "application/vnd.openxmlformats-officedocument.presentationml.commentAuthors+xml" => 1237u16,
        "application/vnd.openxmlformats-officedocument.presentationml.comments+xml" => 1238u16,
        "application/vnd.openxmlformats-officedocument.presentationml.handoutMaster+xml" => 1239u16,
        "application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml" => 1240u16,
        "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml" => 1241u16,
        "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml" => 1242u16,
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => 1243u16,
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml" => 1244u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slide" => 1245u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slide+xml" => 1246u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml" => 1247u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml" => 1248u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slideUpdateInfo+xml" => 1249u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slideshow" => 1250u16,
        "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml" => 1251u16,
        "application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml" => 1252u16,
        "application/vnd.openxmlformats-officedocument.presentationml.tags+xml" => 1253u16,
        "application/vnd.openxmlformats-officedocument.presentationml.template" => 1254u16,
        "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml" => 1255u16,
        "application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml" => 1256u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml" => 1257u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml" => 1258u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml" => 1259u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml" => 1260u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.dialogsheet+xml" => 1261u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.externalLink+xml" => 1262u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml" => 1263u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml" => 1264u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml" => 1265u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.queryTable+xml" => 1266u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionHeaders+xml" => 1267u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionLog+xml" => 1268u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml" => 1269u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => 1270u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml" => 1271u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml" => 1272u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml" => 1273u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml" => 1274u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.tableSingleCells+xml" => 1275u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.template" => 1276u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml" => 1277u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.userNames+xml" => 1278u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.volatileDependencies+xml" => 1279u16,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml" => 1280u16,
        "application/vnd.openxmlformats-officedocument.theme+xml" => 1281u16,
        "application/vnd.openxmlformats-officedocument.themeOverride+xml" => 1282u16,
        "application/vnd.openxmlformats-officedocument.vmlDrawing" => 1283u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml" => 1284u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => 1285u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.glossary+xml" => 1286u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml" => 1287u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml" => 1288u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml" => 1289u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml" => 1290u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml" => 1291u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml" => 1292u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml" => 1293u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml" => 1294u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.template" => 1295u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml" => 1296u16,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.webSettings+xml" => 1297u16,
        "application/vnd.openxmlformats-package.core-properties+xml" => 1298u16,
        "application/vnd.openxmlformats-package.digital-signature-xmlsignature+xml" => 1299u16,
        "application/vnd.openxmlformats-package.relationships+xml" => 1300u16,
        "application/vnd.oracle.resource+json" => 1301u16,
        "application/vnd.orange.indata" => 1302u16,
        "application/vnd.osa.netdeploy" => 1303u16,
        "application/vnd.osgeo.mapguide.package" => 1304u16,
        "application/vnd.osgi.bundle" => 1305u16,
        "application/vnd.osgi.dp" => 1306u16,
        "application/vnd.osgi.subsystem" => 1307u16,
        "application/vnd.otps.ct-kip+xml" => 1308u16,
        "application/vnd.oxli.countgraph" => 1309u16,
        "application/vnd.pagerduty+json" => 1310u16,
        "application/vnd.palm" => 1311u16,
        "application/vnd.panoply" => 1312u16,
        "application/vnd.paos.xml" => 1313u16,
        "application/vnd.patentdive" => 1314u16,
        "application/vnd.patientecommsdoc" => 1315u16,
        "application/vnd.pawaafile" => 1316u16,
        "application/vnd.pcos" => 1317u16,
        "application/vnd.pg.format" => 1318u16,
        "application/vnd.pg.osasli" => 1319u16,
        "application/vnd.piaccess.application-licence" => 1320u16,
        "application/vnd.picsel" => 1321u16,
        "application/vnd.pmi.widget" => 1322u16,
        "application/vnd.poc.group-advertisement+xml" => 1323u16,
        "application/vnd.pocketlearn" => 1324u16,
        "application/vnd.powerbuilder6" => 1325u16,
        "application/vnd.powerbuilder6-s" => 1326u16,
        "application/vnd.powerbuilder7" => 1327u16,
        "application/vnd.powerbuilder7-s" => 1328u16,
        "application/vnd.powerbuilder75" => 1329u16,
        "application/vnd.powerbuilder75-s" => 1330u16,
        "application/vnd.preminet" => 1331u16,
        "application/vnd.previewsystems.box" => 1332u16,
        "application/vnd.proteus.magazine" => 1333u16,
        "application/vnd.psfs" => 1334u16,
        "application/vnd.pt.mundusmundi" => 1335u16,
        "application/vnd.publishare-delta-tree" => 1336u16,
        "application/vnd.pvi.ptid1" => 1337u16,
        "application/vnd.pwg-multiplexed" => 1338u16,
        "application/vnd.pwg-xhtml-print+xml" => 1339u16,
        "application/vnd.qualcomm.brew-app-res" => 1340u16,
        "application/vnd.quarantainenet" => 1341u16,
        "application/vnd.quobject-quoxdocument" => 1342u16,
        "application/vnd.radisys.moml+xml" => 1343u16,
        "application/vnd.radisys.msml+xml" => 1344u16,
        "application/vnd.radisys.msml-audit+xml" => 1345u16,
        "application/vnd.radisys.msml-audit-conf+xml" => 1346u16,
        "application/vnd.radisys.msml-audit-conn+xml" => 1347u16,
        "application/vnd.radisys.msml-audit-dialog+xml" => 1348u16,
        "application/vnd.radisys.msml-audit-stream+xml" => 1349u16,
        "application/vnd.radisys.msml-conf+xml" => 1350u16,
        "application/vnd.radisys.msml-dialog+xml" => 1351u16,
        "application/vnd.radisys.msml-dialog-base+xml" => 1352u16,
        "application/vnd.radisys.msml-dialog-fax-detect+xml" => 1353u16,
        "application/vnd.radisys.msml-dialog-fax-sendrecv+xml" => 1354u16,
        "application/vnd.radisys.msml-dialog-group+xml" => 1355u16,
        "application/vnd.radisys.msml-dialog-speech+xml" => 1356u16,
        "application/vnd.radisys.msml-dialog-transform+xml" => 1357u16,
        "application/vnd.rainstor.data" => 1358u16,
        "application/vnd.rapid" => 1359u16,
        "application/vnd.rar" => 1360u16,
        "application/vnd.realvnc.bed" => 1361u16,
        "application/vnd.recordare.musicxml" => 1362u16,
        "application/vnd.recordare.musicxml+xml" => 1363u16,
        "application/vnd.relpipe" => 1364u16,
        "application/vnd.resilient.logic" => 1365u16,
        "application/vnd.restful+json" => 1366u16,
        "application/vnd.rig.cryptonote" => 1367u16,
        "application/vnd.route66.link66+xml" => 1368u16,
        "application/vnd.rs-274x" => 1369u16,
        "application/vnd.ruckus.download" => 1370u16,
        "application/vnd.s3sms" => 1371u16,
        "application/vnd.sailingtracker.track" => 1372u16,
        "application/vnd.sar" => 1373u16,
        "application/vnd.sbm.cid" => 1374u16,
        "application/vnd.sbm.mid2" => 1375u16,
        "application/vnd.scribus" => 1376u16,
        "application/vnd.sealed.3df" => 1377u16,
        "application/vnd.sealed.csf" => 1378u16,
        "application/vnd.sealed.doc" => 1379u16,
        "application/vnd.sealed.eml" => 1380u16,
        "application/vnd.sealed.mht" => 1381u16,
        "application/vnd.sealed.net" => 1382u16,
        "application/vnd.sealed.ppt" => 1383u16,
        "application/vnd.sealed.tiff" => 1384u16,
        "application/vnd.sealed.xls" => 1385u16,
        "application/vnd.sealedmedia.softseal.html" => 1386u16,
        "application/vnd.sealedmedia.softseal.pdf" => 1387u16,
        "application/vnd.seemail" => 1388u16,
        "application/vnd.seis+json" => 1389u16,
        "application/vnd.sema" => 1390u16,
        "application/vnd.semd" => 1391u16,
        "application/vnd.semf" => 1392u16,
        "application/vnd.shade-save-file" => 1393u16,
        "application/vnd.shana.informed.formdata" => 1394u16,
        "application/vnd.shana.informed.formtemplate" => 1395u16,
        "application/vnd.shana.informed.interchange" => 1396u16,
        "application/vnd.shana.informed.package" => 1397u16,
        "application/vnd.shootproof+json" => 1398u16,
        "application/vnd.shopkick+json" => 1399u16,
        "application/vnd.shp" => 1400u16,
        "application/vnd.shx" => 1401u16,
        "application/vnd.sigrok.session" => 1402u16,
        "application/vnd.siren+json" => 1403u16,
        "application/vnd.smaf" => 1404u16,
        "application/vnd.smart.notebook" => 1405u16,
        "application/vnd.smart.teacher" => 1406u16,
        "application/vnd.smintio.portals.archive" => 1407u16,
        "application/vnd.snesdev-page-table" => 1408u16,
        "application/vnd.software602.filler.form+xml" => 1409u16,
        "application/vnd.software602.filler.form-xml-zip" => 1410u16,
        "application/vnd.solent.sdkm+xml" => 1411u16,
        "application/vnd.spotfire.dxp" => 1412u16,
        "application/vnd.spotfire.sfs" => 1413u16,
        "application/vnd.sqlite3" => 1414u16,
        "application/vnd.sss-cod" => 1415u16,
        "application/vnd.sss-dtf" => 1416u16,
        "application/vnd.sss-ntf" => 1417u16,
        "application/vnd.stepmania.package" => 1418u16,
        "application/vnd.stepmania.stepchart" => 1419u16,
        "application/vnd.street-stream" => 1420u16,
        "application/vnd.sun.wadl+xml" => 1421u16,
        "application/vnd.sus-calendar" => 1422u16,
        "application/vnd.svd" => 1423u16,
        "application/vnd.swiftview-ics" => 1424u16,
        "application/vnd.sybyl.mol2" => 1425u16,
        "application/vnd.sycle+xml" => 1426u16,
        "application/vnd.syft+json" => 1427u16,
        "application/vnd.syncml+xml" => 1428u16,
        "application/vnd.syncml.dm+wbxml" => 1429u16,
        "application/vnd.syncml.dm+xml" => 1430u16,
        "application/vnd.syncml.dm.notification" => 1431u16,
        "application/vnd.syncml.dmddf+wbxml" => 1432u16,
        "application/vnd.syncml.dmddf+xml" => 1433u16,
        "application/vnd.syncml.dmtnds+wbxml" => 1434u16,
        "application/vnd.syncml.dmtnds+xml" => 1435u16,
        "application/vnd.syncml.ds.notification" => 1436u16,
        "application/vnd.tableschema+json" => 1437u16,
        "application/vnd.tao.intent-module-archive" => 1438u16,
        "application/vnd.tcpdump.pcap" => 1439u16,
        "application/vnd.think-cell.ppttc+json" => 1440u16,
        "application/vnd.tmd.mediaflex.api+xml" => 1441u16,
        "application/vnd.tml" => 1442u16,
        "application/vnd.tmobile-livetv" => 1443u16,
        "application/vnd.tri.onesource" => 1444u16,
        "application/vnd.trid.tpt" => 1445u16,
        "application/vnd.triscape.mxs" => 1446u16,
        "application/vnd.trueapp" => 1447u16,
        "application/vnd.truedoc" => 1448u16,
        "application/vnd.ubisoft.webplayer" => 1449u16,
        "application/vnd.ufdl" => 1450u16,
        "application/vnd.uiq.theme" => 1451u16,
        "application/vnd.umajin" => 1452u16,
        "application/vnd.unity" => 1453u16,
        "application/vnd.uoml+xml" => 1454u16,
        "application/vnd.uplanet.alert" => 1455u16,
        "application/vnd.uplanet.alert-wbxml" => 1456u16,
        "application/vnd.uplanet.bearer-choice" => 1457u16,
        "application/vnd.uplanet.bearer-choice-wbxml" => 1458u16,
        "application/vnd.uplanet.cacheop" => 1459u16,
        "application/vnd.uplanet.cacheop-wbxml" => 1460u16,
        "application/vnd.uplanet.channel" => 1461u16,
        "application/vnd.uplanet.channel-wbxml" => 1462u16,
        "application/vnd.uplanet.list" => 1463u16,
        "application/vnd.uplanet.list-wbxml" => 1464u16,
        "application/vnd.uplanet.listcmd" => 1465u16,
        "application/vnd.uplanet.listcmd-wbxml" => 1466u16,
        "application/vnd.uplanet.signal" => 1467u16,
        "application/vnd.uri-map" => 1468u16,
        "application/vnd.valve.source.material" => 1469u16,
        "application/vnd.vcx" => 1470u16,
        "application/vnd.vd-study" => 1471u16,
        "application/vnd.vectorworks" => 1472u16,
        "application/vnd.vel+json" => 1473u16,
        "application/vnd.verimatrix.vcas" => 1474u16,
        "application/vnd.veritone.aion+json" => 1475u16,
        "application/vnd.veryant.thin" => 1476u16,
        "application/vnd.ves.encrypted" => 1477u16,
        "application/vnd.vidsoft.vidconference" => 1478u16,
        "application/vnd.visio" => 1479u16,
        "application/vnd.visionary" => 1480u16,
        "application/vnd.vividence.scriptfile" => 1481u16,
        "application/vnd.vsf" => 1482u16,
        "application/vnd.wap.sic" => 1483u16,
        "application/vnd.wap.slc" => 1484u16,
        "application/vnd.wap.wbxml" => 1485u16,
        "application/vnd.wap.wmlc" => 1486u16,
        "application/vnd.wap.wmlscriptc" => 1487u16,
        "application/vnd.wasmflow.wafl" => 1488u16,
        "application/vnd.webturbo" => 1489u16,
        "application/vnd.wfa.dpp" => 1490u16,
        "application/vnd.wfa.p2p" => 1491u16,
        "application/vnd.wfa.wsc" => 1492u16,
        "application/vnd.windows.devicepairing" => 1493u16,
        "application/vnd.wmc" => 1494u16,
        "application/vnd.wmf.bootstrap" => 1495u16,
        "application/vnd.wolfram.mathematica" => 1496u16,
        "application/vnd.wolfram.mathematica.package" => 1497u16,
        "application/vnd.wolfram.player" => 1498u16,
        "application/vnd.wordlift" => 1499u16,
        "application/vnd.wordperfect" => 1500u16,
        "application/vnd.wqd" => 1501u16,
        "application/vnd.wrq-hp3000-labelled" => 1502u16,
        "application/vnd.wt.stf" => 1503u16,
        "application/vnd.wv.csp+wbxml" => 1504u16,
        "application/vnd.wv.csp+xml" => 1505u16,
        "application/vnd.wv.ssp+xml" => 1506u16,
        "application/vnd.xacml+json" => 1507u16,
        "application/vnd.xara" => 1508u16,
        "application/vnd.xecrets-encrypted" => 1509u16,
        "application/vnd.xfdl" => 1510u16,
        "application/vnd.xfdl.webform" => 1511u16,
        "application/vnd.xmi+xml" => 1512u16,
        "application/vnd.xmpie.cpkg" => 1513u16,
        "application/vnd.xmpie.dpkg" => 1514u16,
        "application/vnd.xmpie.plan" => 1515u16,
        "application/vnd.xmpie.ppkg" => 1516u16,
        "application/vnd.xmpie.xlim" => 1517u16,
        "application/vnd.yamaha.hv-dic" => 1518u16,
        "application/vnd.yamaha.hv-script" => 1519u16,
        "application/vnd.yamaha.hv-voice" => 1520u16,
        "application/vnd.yamaha.openscoreformat" => 1521u16,
        "application/vnd.yamaha.openscoreformat.osfpvg+xml" => 1522u16,
        "application/vnd.yamaha.remote-setup" => 1523u16,
        "application/vnd.yamaha.smaf-audio" => 1524u16,
        "application/vnd.yamaha.smaf-phrase" => 1525u16,
        "application/vnd.yamaha.through-ngn" => 1526u16,
        "application/vnd.yamaha.tunnel-udpencap" => 1527u16,
        "application/vnd.yaoweme" => 1528u16,
        "application/vnd.yellowriver-custom-menu" => 1529u16,
        "application/vnd.youtube.yt" => 1530u16,
        "application/vnd.zul" => 1531u16,
        "application/vnd.zzazz.deck+xml" => 1532u16,
        "application/voicexml+xml" => 1533u16,
        "application/voucher-cms+json" => 1534u16,
        "application/vq-rtcpxr" => 1535u16,
        "application/wasm" => 1536u16,
        "application/watcherinfo+xml" => 1537u16,
        "application/webpush-options+json" => 1538u16,
        "application/whoispp-query" => 1539u16,
        "application/whoispp-response" => 1540u16,
        "application/widget" => 1541u16,
        "application/wita" => 1542u16,
        "application/wordperfect5.1" => 1543u16,
        "application/wsdl+xml" => 1544u16,
        "application/wspolicy+xml" => 1545u16,
        "application/x-pki-message" => 1546u16,
        "application/x-www-form-urlencoded" => 1547u16,
        "application/x-x509-ca-cert" => 1548u16,
        "application/x-x509-ca-ra-cert" => 1549u16,
        "application/x-x509-next-ca-cert" => 1550u16,
        "application/x400-bp" => 1551u16,
        "application/xacml+xml" => 1552u16,
        "application/xcap-att+xml" => 1553u16,
        "application/xcap-caps+xml" => 1554u16,
        "application/xcap-diff+xml" => 1555u16,
        "application/xcap-el+xml" => 1556u16,
        "application/xcap-error+xml" => 1557u16,
        "application/xcap-ns+xml" => 1558u16,
        "application/xcon-conference-info+xml" => 1559u16,
        "application/xcon-conference-info-diff+xml" => 1560u16,
        "application/xenc+xml" => 1561u16,
        "application/xfdf" => 1562u16,
        "application/xhtml+xml" => 1563u16,
        "application/xliff+xml" => 1564u16,
        "application/xml" => 1565u16,
        "application/xml-dtd" => 1566u16,
        "application/xml-external-parsed-entity" => 1567u16,
        "application/xml-patch+xml" => 1568u16,
        "application/xmpp+xml" => 1569u16,
        "application/xop+xml" => 1570u16,
        "application/xslt+xml" => 1571u16,
        "application/xv+xml" => 1572u16,
        "application/yaml" => 1573u16,
        "application/yang" => 1574u16,
        "application/yang-data+cbor" => 1575u16,
        "application/yang-data+json" => 1576u16,
        "application/yang-data+xml" => 1577u16,
        "application/yang-patch+json" => 1578u16,
        "application/yang-patch+xml" => 1579u16,
        "application/yang-sid+json" => 1580u16,
        "application/yin+xml" => 1581u16,
        "application/zip" => 1582u16,
        "application/zlib" => 1583u16,
        "application/zstd" => 1584u16,
        "audio/1d-interleaved-parityfec" => 1585u16,
        "audio/32kadpcm" => 1586u16,
        "audio/3gpp" => 1587u16,
        "audio/3gpp2" => 1588u16,
        "audio/AMR" => 1589u16,
        "audio/AMR-WB" => 1590u16,
        "audio/ATRAC-ADVANCED-LOSSLESS" => 1591u16,
        "audio/ATRAC-X" => 1592u16,
        "audio/ATRAC3" => 1593u16,
        "audio/BV16" => 1594u16,
        "audio/BV32" => 1595u16,
        "audio/CN" => 1596u16,
        "audio/DAT12" => 1597u16,
        "audio/DV" => 1598u16,
        "audio/DVI4" => 1599u16,
        "audio/EVRC" => 1600u16,
        "audio/EVRC-QCP" => 1601u16,
        "audio/EVRC0" => 1602u16,
        "audio/EVRC1" => 1603u16,
        "audio/EVRCB" => 1604u16,
        "audio/EVRCB0" => 1605u16,
        "audio/EVRCB1" => 1606u16,
        "audio/EVRCNW" => 1607u16,
        "audio/EVRCNW0" => 1608u16,
        "audio/EVRCNW1" => 1609u16,
        "audio/EVRCWB" => 1610u16,
        "audio/EVRCWB0" => 1611u16,
        "audio/EVRCWB1" => 1612u16,
        "audio/EVS" => 1613u16,
        "audio/G711-0" => 1614u16,
        "audio/G719" => 1615u16,
        "audio/G722" => 1616u16,
        "audio/G7221" => 1617u16,
        "audio/G723" => 1618u16,
        "audio/G726-16" => 1619u16,
        "audio/G726-24" => 1620u16,
        "audio/G726-32" => 1621u16,
        "audio/G726-40" => 1622u16,
        "audio/G728" => 1623u16,
        "audio/G729" => 1624u16,
        "audio/G7291" => 1625u16,
        "audio/G729D" => 1626u16,
        "audio/G729E" => 1627u16,
        "audio/GSM" => 1628u16,
        "audio/GSM-EFR" => 1629u16,
        "audio/GSM-HR-08" => 1630u16,
        "audio/L16" => 1631u16,
        "audio/L20" => 1632u16,
        "audio/L24" => 1633u16,
        "audio/L8" => 1634u16,
        "audio/LPC" => 1635u16,
        "audio/MELP" => 1636u16,
        "audio/MELP1200" => 1637u16,
        "audio/MELP2400" => 1638u16,
        "audio/MELP600" => 1639u16,
        "audio/MP4A-LATM" => 1640u16,
        "audio/MPA" => 1641u16,
        "audio/PCMA" => 1642u16,
        "audio/PCMA-WB" => 1643u16,
        "audio/PCMU" => 1644u16,
        "audio/PCMU-WB" => 1645u16,
        "audio/QCELP" => 1646u16,
        "audio/RED" => 1647u16,
        "audio/SMV" => 1648u16,
        "audio/SMV-QCP" => 1649u16,
        "audio/SMV0" => 1650u16,
        "audio/TETRA_ACELP" => 1651u16,
        "audio/TETRA_ACELP_BB" => 1652u16,
        "audio/TSVCIS" => 1653u16,
        "audio/UEMCLIP" => 1654u16,
        "audio/VDVI" => 1655u16,
        "audio/VMR-WB" => 1656u16,
        "audio/aac" => 1657u16,
        "audio/ac3" => 1658u16,
        "audio/amr-wb+" => 1659u16,
        "audio/aptx" => 1660u16,
        "audio/asc" => 1661u16,
        "audio/basic" => 1662u16,
        "audio/clearmode" => 1663u16,
        "audio/dls" => 1664u16,
        "audio/dsr-es201108" => 1665u16,
        "audio/dsr-es202050" => 1666u16,
        "audio/dsr-es202211" => 1667u16,
        "audio/dsr-es202212" => 1668u16,
        "audio/eac3" => 1669u16,
        "audio/encaprtp" => 1670u16,
        "audio/example" => 1671u16,
        "audio/flexfec" => 1672u16,
        "audio/fwdred" => 1673u16,
        "audio/iLBC" => 1674u16,
        "audio/ip-mr_v2.5" => 1675u16,
        "audio/matroska" => 1676u16,
        "audio/mhas" => 1677u16,
        "audio/mobile-xmf" => 1678u16,
        "audio/mp4" => 1679u16,
        "audio/mpa-robust" => 1680u16,
        "audio/mpeg" => 1681u16,
        "audio/mpeg4-generic" => 1682u16,
        "audio/ogg" => 1683u16,
        "audio/opus" => 1684u16,
        "audio/parityfec" => 1685u16,
        "audio/prs.sid" => 1686u16,
        "audio/raptorfec" => 1687u16,
        "audio/rtp-enc-aescm128" => 1688u16,
        "audio/rtp-midi" => 1689u16,
        "audio/rtploopback" => 1690u16,
        "audio/rtx" => 1691u16,
        "audio/scip" => 1692u16,
        "audio/sofa" => 1693u16,
        "audio/sp-midi" => 1694u16,
        "audio/speex" => 1695u16,
        "audio/t140c" => 1696u16,
        "audio/t38" => 1697u16,
        "audio/telephone-event" => 1698u16,
        "audio/tone" => 1699u16,
        "audio/ulpfec" => 1700u16,
        "audio/usac" => 1701u16,
        "audio/vnd.3gpp.iufp" => 1702u16,
        "audio/vnd.4SB" => 1703u16,
        "audio/vnd.CELP" => 1704u16,
        "audio/vnd.audiokoz" => 1705u16,
        "audio/vnd.cisco.nse" => 1706u16,
        "audio/vnd.cmles.radio-events" => 1707u16,
        "audio/vnd.cns.anp1" => 1708u16,
        "audio/vnd.cns.inf1" => 1709u16,
        "audio/vnd.dece.audio" => 1710u16,
        "audio/vnd.digital-winds" => 1711u16,
        "audio/vnd.dlna.adts" => 1712u16,
        "audio/vnd.dolby.heaac.1" => 1713u16,
        "audio/vnd.dolby.heaac.2" => 1714u16,
        "audio/vnd.dolby.mlp" => 1715u16,
        "audio/vnd.dolby.mps" => 1716u16,
        "audio/vnd.dolby.pl2" => 1717u16,
        "audio/vnd.dolby.pl2x" => 1718u16,
        "audio/vnd.dolby.pl2z" => 1719u16,
        "audio/vnd.dolby.pulse.1" => 1720u16,
        "audio/vnd.dra" => 1721u16,
        "audio/vnd.dts" => 1722u16,
        "audio/vnd.dts.hd" => 1723u16,
        "audio/vnd.dts.uhd" => 1724u16,
        "audio/vnd.dvb.file" => 1725u16,
        "audio/vnd.everad.plj" => 1726u16,
        "audio/vnd.hns.audio" => 1727u16,
        "audio/vnd.lucent.voice" => 1728u16,
        "audio/vnd.ms-playready.media.pya" => 1729u16,
        "audio/vnd.nokia.mobile-xmf" => 1730u16,
        "audio/vnd.nortel.vbk" => 1731u16,
        "audio/vnd.nuera.ecelp4800" => 1732u16,
        "audio/vnd.nuera.ecelp7470" => 1733u16,
        "audio/vnd.nuera.ecelp9600" => 1734u16,
        "audio/vnd.octel.sbc" => 1735u16,
        "audio/vnd.presonus.multitrack" => 1736u16,
        "audio/vnd.qcelp" => 1737u16,
        "audio/vnd.rhetorex.32kadpcm" => 1738u16,
        "audio/vnd.rip" => 1739u16,
        "audio/vnd.sealedmedia.softseal.mpeg" => 1740u16,
        "audio/vnd.vmx.cvsd" => 1741u16,
        "audio/vorbis" => 1742u16,
        "audio/vorbis-config" => 1743u16,
        "font/collection" => 1744u16,
        "font/otf" => 1745u16,
        "font/sfnt" => 1746u16,
        "font/ttf" => 1747u16,
        "font/woff" => 1748u16,
        "font/woff2" => 1749u16,
        "image/aces" => 1750u16,
        "image/apng" => 1751u16,
        "image/avci" => 1752u16,
        "image/avcs" => 1753u16,
        "image/avif" => 1754u16,
        "image/bmp" => 1755u16,
        "image/cgm" => 1756u16,
        "image/dicom-rle" => 1757u16,
        "image/dpx" => 1758u16,
        "image/emf" => 1759u16,
        "image/example" => 1760u16,
        "image/fits" => 1761u16,
        "image/g3fax" => 1762u16,
        "image/gif" => 1763u16,
        "image/heic" => 1764u16,
        "image/heic-sequence" => 1765u16,
        "image/heif" => 1766u16,
        "image/heif-sequence" => 1767u16,
        "image/hej2k" => 1768u16,
        "image/hsj2" => 1769u16,
        "image/ief" => 1770u16,
        "image/j2c" => 1771u16,
        "image/jls" => 1772u16,
        "image/jp2" => 1773u16,
        "image/jpeg" => 1774u16,
        "image/jph" => 1775u16,
        "image/jphc" => 1776u16,
        "image/jpm" => 1777u16,
        "image/jpx" => 1778u16,
        "image/jxr" => 1779u16,
        "image/jxrA" => 1780u16,
        "image/jxrS" => 1781u16,
        "image/jxs" => 1782u16,
        "image/jxsc" => 1783u16,
        "image/jxsi" => 1784u16,
        "image/jxss" => 1785u16,
        "image/ktx" => 1786u16,
        "image/ktx2" => 1787u16,
        "image/naplps" => 1788u16,
        "image/png" => 1789u16,
        "image/prs.btif" => 1790u16,
        "image/prs.pti" => 1791u16,
        "image/pwg-raster" => 1792u16,
        "image/svg+xml" => 1793u16,
        "image/t38" => 1794u16,
        "image/tiff" => 1795u16,
        "image/tiff-fx" => 1796u16,
        "image/vnd.adobe.photoshop" => 1797u16,
        "image/vnd.airzip.accelerator.azv" => 1798u16,
        "image/vnd.cns.inf2" => 1799u16,
        "image/vnd.dece.graphic" => 1800u16,
        "image/vnd.djvu" => 1801u16,
        "image/vnd.dvb.subtitle" => 1802u16,
        "image/vnd.dwg" => 1803u16,
        "image/vnd.dxf" => 1804u16,
        "image/vnd.fastbidsheet" => 1805u16,
        "image/vnd.fpx" => 1806u16,
        "image/vnd.fst" => 1807u16,
        "image/vnd.fujixerox.edmics-mmr" => 1808u16,
        "image/vnd.fujixerox.edmics-rlc" => 1809u16,
        "image/vnd.globalgraphics.pgb" => 1810u16,
        "image/vnd.microsoft.icon" => 1811u16,
        "image/vnd.mix" => 1812u16,
        "image/vnd.mozilla.apng" => 1813u16,
        "image/vnd.ms-modi" => 1814u16,
        "image/vnd.net-fpx" => 1815u16,
        "image/vnd.pco.b16" => 1816u16,
        "image/vnd.radiance" => 1817u16,
        "image/vnd.sealed.png" => 1818u16,
        "image/vnd.sealedmedia.softseal.gif" => 1819u16,
        "image/vnd.sealedmedia.softseal.jpg" => 1820u16,
        "image/vnd.svf" => 1821u16,
        "image/vnd.tencent.tap" => 1822u16,
        "image/vnd.valve.source.texture" => 1823u16,
        "image/vnd.wap.wbmp" => 1824u16,
        "image/vnd.xiff" => 1825u16,
        "image/vnd.zbrush.pcx" => 1826u16,
        "image/webp" => 1827u16,
        "image/wmf" => 1828u16,
        "message/CPIM" => 1829u16,
        "message/bhttp" => 1830u16,
        "message/delivery-status" => 1831u16,
        "message/disposition-notification" => 1832u16,
        "message/example" => 1833u16,
        "message/external-body" => 1834u16,
        "message/feedback-report" => 1835u16,
        "message/global" => 1836u16,
        "message/global-delivery-status" => 1837u16,
        "message/global-disposition-notification" => 1838u16,
        "message/global-headers" => 1839u16,
        "message/http" => 1840u16,
        "message/imdn+xml" => 1841u16,
        "message/mls" => 1842u16,
        "message/news" => 1843u16,
        "message/ohttp-req" => 1844u16,
        "message/ohttp-res" => 1845u16,
        "message/partial" => 1846u16,
        "message/rfc822" => 1847u16,
        "message/s-http" => 1848u16,
        "message/sip" => 1849u16,
        "message/sipfrag" => 1850u16,
        "message/tracking-status" => 1851u16,
        "message/vnd.si.simp" => 1852u16,
        "message/vnd.wfa.wsc" => 1853u16,
        "model/3mf" => 1854u16,
        "model/JT" => 1855u16,
        "model/e57" => 1856u16,
        "model/example" => 1857u16,
        "model/gltf+json" => 1858u16,
        "model/gltf-binary" => 1859u16,
        "model/iges" => 1860u16,
        "model/mesh" => 1861u16,
        "model/mtl" => 1862u16,
        "model/obj" => 1863u16,
        "model/prc" => 1864u16,
        "model/step" => 1865u16,
        "model/step+xml" => 1866u16,
        "model/step+zip" => 1867u16,
        "model/step-xml+zip" => 1868u16,
        "model/stl" => 1869u16,
        "model/u3d" => 1870u16,
        "model/vnd.bary" => 1871u16,
        "model/vnd.cld" => 1872u16,
        "model/vnd.collada+xml" => 1873u16,
        "model/vnd.dwf" => 1874u16,
        "model/vnd.flatland.3dml" => 1875u16,
        "model/vnd.gdl" => 1876u16,
        "model/vnd.gs-gdl" => 1877u16,
        "model/vnd.gtw" => 1878u16,
        "model/vnd.moml+xml" => 1879u16,
        "model/vnd.mts" => 1880u16,
        "model/vnd.opengex" => 1881u16,
        "model/vnd.parasolid.transmit.binary" => 1882u16,
        "model/vnd.parasolid.transmit.text" => 1883u16,
        "model/vnd.pytha.pyox" => 1884u16,
        "model/vnd.rosette.annotated-data-model" => 1885u16,
        "model/vnd.sap.vds" => 1886u16,
        "model/vnd.usda" => 1887u16,
        "model/vnd.usdz+zip" => 1888u16,
        "model/vnd.valve.source.compiled-map" => 1889u16,
        "model/vnd.vtu" => 1890u16,
        "model/vrml" => 1891u16,
        "model/x3d+fastinfoset" => 1892u16,
        "model/x3d+xml" => 1893u16,
        "model/x3d-vrml" => 1894u16,
        "multipart/alternative" => 1895u16,
        "multipart/appledouble" => 1896u16,
        "multipart/byteranges" => 1897u16,
        "multipart/digest" => 1898u16,
        "multipart/encrypted" => 1899u16,
        "multipart/example" => 1900u16,
        "multipart/form-data" => 1901u16,
        "multipart/header-set" => 1902u16,
        "multipart/mixed" => 1903u16,
        "multipart/multilingual" => 1904u16,
        "multipart/parallel" => 1905u16,
        "multipart/related" => 1906u16,
        "multipart/report" => 1907u16,
        "multipart/signed" => 1908u16,
        "multipart/vnd.bint.med-plus" => 1909u16,
        "multipart/voice-message" => 1910u16,
        "multipart/x-mixed-replace" => 1911u16,
        "text/1d-interleaved-parityfec" => 1912u16,
        "text/RED" => 1913u16,
        "text/SGML" => 1914u16,
        "text/cache-manifest" => 1915u16,
        "text/calendar" => 1916u16,
        "text/cql" => 1917u16,
        "text/cql-expression" => 1918u16,
        "text/cql-identifier" => 1919u16,
        "text/css" => 1920u16,
        "text/csv" => 1921u16,
        "text/csv-schema" => 1922u16,
        "text/directory" => 1923u16,
        "text/dns" => 1924u16,
        "text/ecmascript" => 1925u16,
        "text/encaprtp" => 1926u16,
        "text/enriched" => 1927u16,
        "text/example" => 1928u16,
        "text/fhirpath" => 1929u16,
        "text/flexfec" => 1930u16,
        "text/fwdred" => 1931u16,
        "text/gff3" => 1932u16,
        "text/grammar-ref-list" => 1933u16,
        "text/hl7v2" => 1934u16,
        "text/html" => 1935u16,
        "text/javascript" => 1936u16,
        "text/jcr-cnd" => 1937u16,
        "text/markdown" => 1938u16,
        "text/mizar" => 1939u16,
        "text/n3" => 1940u16,
        "text/parameters" => 1941u16,
        "text/parityfec" => 1942u16,
        "text/plain" => 1943u16,
        "text/provenance-notation" => 1944u16,
        "text/prs.fallenstein.rst" => 1945u16,
        "text/prs.lines.tag" => 1946u16,
        "text/prs.prop.logic" => 1947u16,
        "text/prs.texi" => 1948u16,
        "text/raptorfec" => 1949u16,
        "text/rfc822-headers" => 1950u16,
        "text/richtext" => 1951u16,
        "text/rtf" => 1952u16,
        "text/rtp-enc-aescm128" => 1953u16,
        "text/rtploopback" => 1954u16,
        "text/rtx" => 1955u16,
        "text/shaclc" => 1956u16,
        "text/shex" => 1957u16,
        "text/spdx" => 1958u16,
        "text/strings" => 1959u16,
        "text/t140" => 1960u16,
        "text/tab-separated-values" => 1961u16,
        "text/troff" => 1962u16,
        "text/turtle" => 1963u16,
        "text/ulpfec" => 1964u16,
        "text/uri-list" => 1965u16,
        "text/vcard" => 1966u16,
        "text/vnd.DMClientScript" => 1967u16,
        "text/vnd.IPTC.NITF" => 1968u16,
        "text/vnd.IPTC.NewsML" => 1969u16,
        "text/vnd.a" => 1970u16,
        "text/vnd.abc" => 1971u16,
        "text/vnd.ascii-art" => 1972u16,
        "text/vnd.curl" => 1973u16,
        "text/vnd.debian.copyright" => 1974u16,
        "text/vnd.dvb.subtitle" => 1975u16,
        "text/vnd.esmertec.theme-descriptor" => 1976u16,
        "text/vnd.exchangeable" => 1977u16,
        "text/vnd.familysearch.gedcom" => 1978u16,
        "text/vnd.ficlab.flt" => 1979u16,
        "text/vnd.fly" => 1980u16,
        "text/vnd.fmi.flexstor" => 1981u16,
        "text/vnd.gml" => 1982u16,
        "text/vnd.graphviz" => 1983u16,
        "text/vnd.hans" => 1984u16,
        "text/vnd.hgl" => 1985u16,
        "text/vnd.in3d.3dml" => 1986u16,
        "text/vnd.in3d.spot" => 1987u16,
        "text/vnd.latex-z" => 1988u16,
        "text/vnd.motorola.reflex" => 1989u16,
        "text/vnd.ms-mediapackage" => 1990u16,
        "text/vnd.net2phone.commcenter.command" => 1991u16,
        "text/vnd.radisys.msml-basic-layout" => 1992u16,
        "text/vnd.senx.warpscript" => 1993u16,
        "text/vnd.si.uricatalogue" => 1994u16,
        "text/vnd.sosi" => 1995u16,
        "text/vnd.sun.j2me.app-descriptor" => 1996u16,
        "text/vnd.trolltech.linguist" => 1997u16,
        "text/vnd.wap.si" => 1998u16,
        "text/vnd.wap.sl" => 1999u16,
        "text/vnd.wap.wml" => 2000u16,
        "text/vnd.wap.wmlscript" => 2001u16,
        "text/vtt" => 2002u16,
        "text/wgsl" => 2003u16,
        "text/xml" => 2004u16,
        "text/xml-external-parsed-entity" => 2005u16,
        "video/1d-interleaved-parityfec" => 2006u16,
        "video/3gpp" => 2007u16,
        "video/3gpp-tt" => 2008u16,
        "video/3gpp2" => 2009u16,
        "video/AV1" => 2010u16,
        "video/BMPEG" => 2011u16,
        "video/BT656" => 2012u16,
        "video/CelB" => 2013u16,
        "video/DV" => 2014u16,
        "video/FFV1" => 2015u16,
        "video/H261" => 2016u16,
        "video/H263" => 2017u16,
        "video/H263-1998" => 2018u16,
        "video/H263-2000" => 2019u16,
        "video/H264" => 2020u16,
        "video/H264-RCDO" => 2021u16,
        "video/H264-SVC" => 2022u16,
        "video/H265" => 2023u16,
        "video/H266" => 2024u16,
        "video/JPEG" => 2025u16,
        "video/MP1S" => 2026u16,
        "video/MP2P" => 2027u16,
        "video/MP2T" => 2028u16,
        "video/MP4V-ES" => 2029u16,
        "video/MPV" => 2030u16,
        "video/SMPTE292M" => 2031u16,
        "video/VP8" => 2032u16,
        "video/VP9" => 2033u16,
        "video/encaprtp" => 2034u16,
        "video/evc" => 2035u16,
        "video/example" => 2036u16,
        "video/flexfec" => 2037u16,
        "video/iso.segment" => 2038u16,
        "video/jpeg2000" => 2039u16,
        "video/jxsv" => 2040u16,
        "video/matroska" => 2041u16,
        "video/matroska-3d" => 2042u16,
        "video/mj2" => 2043u16,
        "video/mp4" => 2044u16,
        "video/mpeg" => 2045u16,
        "video/mpeg4-generic" => 2046u16,
        "video/nv" => 2047u16,
        "video/ogg" => 2048u16,
        "video/parityfec" => 2049u16,
        "video/pointer" => 2050u16,
        "video/quicktime" => 2051u16,
        "video/raptorfec" => 2052u16,
        "video/raw" => 2053u16,
        "video/rtp-enc-aescm128" => 2054u16,
        "video/rtploopback" => 2055u16,
        "video/rtx" => 2056u16,
        "video/scip" => 2057u16,
        "video/smpte291" => 2058u16,
        "video/ulpfec" => 2059u16,
        "video/vc1" => 2060u16,
        "video/vc2" => 2061u16,
        "video/vnd.CCTV" => 2062u16,
        "video/vnd.dece.hd" => 2063u16,
        "video/vnd.dece.mobile" => 2064u16,
        "video/vnd.dece.mp4" => 2065u16,
        "video/vnd.dece.pd" => 2066u16,
        "video/vnd.dece.sd" => 2067u16,
        "video/vnd.dece.video" => 2068u16,
        "video/vnd.directv.mpeg" => 2069u16,
        "video/vnd.directv.mpeg-tts" => 2070u16,
        "video/vnd.dlna.mpeg-tts" => 2071u16,
        "video/vnd.dvb.file" => 2072u16,
        "video/vnd.fvt" => 2073u16,
        "video/vnd.hns.video" => 2074u16,
        "video/vnd.iptvforum.1dparityfec-1010" => 2075u16,
        "video/vnd.iptvforum.1dparityfec-2005" => 2076u16,
        "video/vnd.iptvforum.2dparityfec-1010" => 2077u16,
        "video/vnd.iptvforum.2dparityfec-2005" => 2078u16,
        "video/vnd.iptvforum.ttsavc" => 2079u16,
        "video/vnd.iptvforum.ttsmpeg2" => 2080u16,
        "video/vnd.motorola.video" => 2081u16,
        "video/vnd.motorola.videop" => 2082u16,
        "video/vnd.mpegurl" => 2083u16,
        "video/vnd.ms-playready.media.pyv" => 2084u16,
        "video/vnd.nokia.interleaved-multimedia" => 2085u16,
        "video/vnd.nokia.mp4vr" => 2086u16,
        "video/vnd.nokia.videovoip" => 2087u16,
        "video/vnd.objectvideo" => 2088u16,
        "video/vnd.radgamettools.bink" => 2089u16,
        "video/vnd.radgamettools.smacker" => 2090u16,
        "video/vnd.sealed.mpeg1" => 2091u16,
        "video/vnd.sealed.mpeg4" => 2092u16,
        "video/vnd.sealed.swf" => 2093u16,
        "video/vnd.sealedmedia.softseal.mov" => 2094u16,
        "video/vnd.uvvu.mp4" => 2095u16,
        "video/vnd.vivo" => 2096u16,
        "video/vnd.youtube.yt" => 2097u16,
    };
} // End of impl

impl EncodingMapping for IanaEncodingMapping {
    /// Given a numerical [`EncodingPrefix`] returns its string representation.
    fn prefix_to_str(&self, p: EncodingPrefix) -> &'static str {
        match Self::KNOWN_PREFIX.get(&p) {
            Some(p) => p,
            None => "unknown",
        }
    }

    /// Given the string representation of a prefix returns its numerical representation as [`EncodingPrefix`].
    /// [EMPTY](`IanaEncodingMapping::EMPTY`) is returned in case of unknown mapping.
    fn str_to_prefix(&self, s: &str) -> EncodingPrefix {
        match Self::KNOWN_STRING.get(s) {
            Some(p) => *p,
            None => Self::EMPTY,
        }
    }

    /// Parse a string into a valid [`Encoding`]. This functions performs the necessary
    /// prefix mapping and suffix substring when parsing the input. In case of unknown prefix mapping,
    /// the [prefix](`Encoding::prefix`) will be set to [EMPTY](`IanaEncodingMapping::EMPTY`) and the
    /// full string will be part of the [suffix](`Encoding::suffix`).
    fn parse<S>(&self, t: S) -> ZResult<Encoding>
    where
        S: Into<Cow<'static, str>>,
    {
        fn _parse(t: Cow<'static, str>) -> ZResult<Encoding> {
            if t.is_empty() {
                return Ok(IanaEncoding::EMPTY);
            }

            // Skip empty string mapping. The order is guaranteed by the phf::OrderedMap.
            for (s, p) in IanaEncodingMapping::KNOWN_STRING.entries().skip(1) {
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
        _parse(t.into())
    }

    /// Given an [`Encoding`] returns a full string representation.
    /// It concatenates the string represenation of the encoding prefix with the encoding suffix.
    fn to_str<'a>(&self, e: &'a Encoding) -> Cow<'a, str> {
        if e.prefix() == IanaEncodingMapping::EMPTY {
            Cow::Borrowed(e.suffix())
        } else {
            Cow::Owned(format!("{}{}", self.prefix_to_str(e.prefix()), e.suffix()))
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct IanaEncoding;

impl IanaEncoding {
    pub const EMPTY: Encoding = Encoding::new(IanaEncodingMapping::EMPTY);
    pub const APPLICATION_1D_INTERLEAVED_PARITYFEC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_1D_INTERLEAVED_PARITYFEC);
    pub const APPLICATION_3GPDASH_QOE_REPORT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_3GPDASH_QOE_REPORT_XML);
    pub const APPLICATION_3GPP_IMS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_3GPP_IMS_XML);
    pub const APPLICATION_3GPPHAL_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_3GPPHAL_JSON);
    pub const APPLICATION_3GPPHALFORMS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_3GPPHALFORMS_JSON);
    pub const APPLICATION_A2L: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_A2L);
    pub const APPLICATION_AML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_AML);
    pub const APPLICATION_ATF: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_ATF);
    pub const APPLICATION_ATFX: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_ATFX);
    pub const APPLICATION_ATXML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_ATXML);
    pub const APPLICATION_CALS_1840: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CALS_1840);
    pub const APPLICATION_CDFX_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CDFX_XML);
    pub const APPLICATION_CEA: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_CEA);
    pub const APPLICATION_CSTADATA_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CSTADATA_XML);
    pub const APPLICATION_DCD: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_DCD);
    pub const APPLICATION_DII: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_DII);
    pub const APPLICATION_DIT: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_DIT);
    pub const APPLICATION_EDI_X12: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EDI_X12);
    pub const APPLICATION_EDI_CONSENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EDI_CONSENT);
    pub const APPLICATION_EDIFACT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EDIFACT);
    pub const APPLICATION_EMERGENCYCALLDATA_COMMENT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMERGENCYCALLDATA_COMMENT_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_CONTROL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMERGENCYCALLDATA_CONTROL_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_DEVICEINFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMERGENCYCALLDATA_DEVICEINFO_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_LEGACYESN_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMERGENCYCALLDATA_LEGACYESN_JSON);
    pub const APPLICATION_EMERGENCYCALLDATA_PROVIDERINFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMERGENCYCALLDATA_PROVIDERINFO_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_SERVICEINFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMERGENCYCALLDATA_SERVICEINFO_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_SUBSCRIBERINFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMERGENCYCALLDATA_SUBSCRIBERINFO_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_VEDS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMERGENCYCALLDATA_VEDS_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_CAP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMERGENCYCALLDATA_CAP_XML);
    pub const APPLICATION_EMERGENCYCALLDATA_ECALL_MSD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMERGENCYCALLDATA_ECALL_MSD);
    pub const APPLICATION_H224: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_H224);
    pub const APPLICATION_IOTP: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_IOTP);
    pub const APPLICATION_ISUP: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_ISUP);
    pub const APPLICATION_LXF: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_LXF);
    pub const APPLICATION_MF4: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_MF4);
    pub const APPLICATION_ODA: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_ODA);
    pub const APPLICATION_ODX: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_ODX);
    pub const APPLICATION_PDX: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_PDX);
    pub const APPLICATION_QSIG: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_QSIG);
    pub const APPLICATION_SGML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_SGML);
    pub const APPLICATION_TETRA_ISI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TETRA_ISI);
    pub const APPLICATION_ACE_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ACE_CBOR);
    pub const APPLICATION_ACE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ACE_JSON);
    pub const APPLICATION_ACTIVEMESSAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ACTIVEMESSAGE);
    pub const APPLICATION_ACTIVITY_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ACTIVITY_JSON);
    pub const APPLICATION_AIF_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_AIF_CBOR);
    pub const APPLICATION_AIF_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_AIF_JSON);
    pub const APPLICATION_ALTO_CDNI_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_CDNI_JSON);
    pub const APPLICATION_ALTO_CDNIFILTER_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_CDNIFILTER_JSON);
    pub const APPLICATION_ALTO_COSTMAP_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_COSTMAP_JSON);
    pub const APPLICATION_ALTO_COSTMAPFILTER_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_COSTMAPFILTER_JSON);
    pub const APPLICATION_ALTO_DIRECTORY_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_DIRECTORY_JSON);
    pub const APPLICATION_ALTO_ENDPOINTCOST_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_ENDPOINTCOST_JSON);
    pub const APPLICATION_ALTO_ENDPOINTCOSTPARAMS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_ENDPOINTCOSTPARAMS_JSON);
    pub const APPLICATION_ALTO_ENDPOINTPROP_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_ENDPOINTPROP_JSON);
    pub const APPLICATION_ALTO_ENDPOINTPROPPARAMS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_ENDPOINTPROPPARAMS_JSON);
    pub const APPLICATION_ALTO_ERROR_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_ERROR_JSON);
    pub const APPLICATION_ALTO_NETWORKMAP_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_NETWORKMAP_JSON);
    pub const APPLICATION_ALTO_NETWORKMAPFILTER_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_NETWORKMAPFILTER_JSON);
    pub const APPLICATION_ALTO_PROPMAP_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_PROPMAP_JSON);
    pub const APPLICATION_ALTO_PROPMAPPARAMS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_PROPMAPPARAMS_JSON);
    pub const APPLICATION_ALTO_TIPS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_TIPS_JSON);
    pub const APPLICATION_ALTO_TIPSPARAMS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_TIPSPARAMS_JSON);
    pub const APPLICATION_ALTO_UPDATESTREAMCONTROL_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_UPDATESTREAMCONTROL_JSON);
    pub const APPLICATION_ALTO_UPDATESTREAMPARAMS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ALTO_UPDATESTREAMPARAMS_JSON);
    pub const APPLICATION_ANDREW_INSET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ANDREW_INSET);
    pub const APPLICATION_APPLEFILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_APPLEFILE);
    pub const APPLICATION_AT_JWT: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_AT_JWT);
    pub const APPLICATION_ATOM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ATOM_XML);
    pub const APPLICATION_ATOMCAT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ATOMCAT_XML);
    pub const APPLICATION_ATOMDELETED_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ATOMDELETED_XML);
    pub const APPLICATION_ATOMICMAIL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ATOMICMAIL);
    pub const APPLICATION_ATOMSVC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ATOMSVC_XML);
    pub const APPLICATION_ATSC_DWD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ATSC_DWD_XML);
    pub const APPLICATION_ATSC_DYNAMIC_EVENT_MESSAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ATSC_DYNAMIC_EVENT_MESSAGE);
    pub const APPLICATION_ATSC_HELD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ATSC_HELD_XML);
    pub const APPLICATION_ATSC_RDT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ATSC_RDT_JSON);
    pub const APPLICATION_ATSC_RSAT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ATSC_RSAT_XML);
    pub const APPLICATION_AUTH_POLICY_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_AUTH_POLICY_XML);
    pub const APPLICATION_AUTOMATIONML_AML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_AUTOMATIONML_AML_XML);
    pub const APPLICATION_AUTOMATIONML_AMLX_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_AUTOMATIONML_AMLX_ZIP);
    pub const APPLICATION_BACNET_XDD_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_BACNET_XDD_ZIP);
    pub const APPLICATION_BATCH_SMTP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_BATCH_SMTP);
    pub const APPLICATION_BEEP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_BEEP_XML);
    pub const APPLICATION_C2PA: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_C2PA);
    pub const APPLICATION_CALENDAR_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CALENDAR_JSON);
    pub const APPLICATION_CALENDAR_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CALENDAR_XML);
    pub const APPLICATION_CALL_COMPLETION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CALL_COMPLETION);
    pub const APPLICATION_CAPTIVE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CAPTIVE_JSON);
    pub const APPLICATION_CBOR: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_CBOR);
    pub const APPLICATION_CBOR_SEQ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CBOR_SEQ);
    pub const APPLICATION_CCCEX: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_CCCEX);
    pub const APPLICATION_CCMP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CCMP_XML);
    pub const APPLICATION_CCXML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CCXML_XML);
    pub const APPLICATION_CDA_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CDA_XML);
    pub const APPLICATION_CDMI_CAPABILITY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CDMI_CAPABILITY);
    pub const APPLICATION_CDMI_CONTAINER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CDMI_CONTAINER);
    pub const APPLICATION_CDMI_DOMAIN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CDMI_DOMAIN);
    pub const APPLICATION_CDMI_OBJECT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CDMI_OBJECT);
    pub const APPLICATION_CDMI_QUEUE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CDMI_QUEUE);
    pub const APPLICATION_CDNI: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_CDNI);
    pub const APPLICATION_CEA_2018_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CEA_2018_XML);
    pub const APPLICATION_CELLML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CELLML_XML);
    pub const APPLICATION_CFW: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_CFW);
    pub const APPLICATION_CID_EDHOC_CBOR_SEQ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CID_EDHOC_CBOR_SEQ);
    pub const APPLICATION_CITY_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CITY_JSON);
    pub const APPLICATION_CLR: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_CLR);
    pub const APPLICATION_CLUE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CLUE_XML);
    pub const APPLICATION_CLUE_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CLUE_INFO_XML);
    pub const APPLICATION_CMS: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_CMS);
    pub const APPLICATION_CNRP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CNRP_XML);
    pub const APPLICATION_COAP_GROUP_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_COAP_GROUP_JSON);
    pub const APPLICATION_COAP_PAYLOAD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_COAP_PAYLOAD);
    pub const APPLICATION_COMMONGROUND: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_COMMONGROUND);
    pub const APPLICATION_CONCISE_PROBLEM_DETAILS_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CONCISE_PROBLEM_DETAILS_CBOR);
    pub const APPLICATION_CONFERENCE_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CONFERENCE_INFO_XML);
    pub const APPLICATION_COSE: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_COSE);
    pub const APPLICATION_COSE_KEY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_COSE_KEY);
    pub const APPLICATION_COSE_KEY_SET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_COSE_KEY_SET);
    pub const APPLICATION_COSE_X509: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_COSE_X509);
    pub const APPLICATION_CPL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CPL_XML);
    pub const APPLICATION_CSRATTRS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CSRATTRS);
    pub const APPLICATION_CSTA_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CSTA_XML);
    pub const APPLICATION_CSVM_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CSVM_JSON);
    pub const APPLICATION_CWL: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_CWL);
    pub const APPLICATION_CWL_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CWL_JSON);
    pub const APPLICATION_CWT: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_CWT);
    pub const APPLICATION_CYBERCASH: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_CYBERCASH);
    pub const APPLICATION_DASH_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DASH_XML);
    pub const APPLICATION_DASH_PATCH_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DASH_PATCH_XML);
    pub const APPLICATION_DASHDELTA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DASHDELTA);
    pub const APPLICATION_DAVMOUNT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DAVMOUNT_XML);
    pub const APPLICATION_DCA_RFT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DCA_RFT);
    pub const APPLICATION_DEC_DX: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_DEC_DX);
    pub const APPLICATION_DIALOG_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DIALOG_INFO_XML);
    pub const APPLICATION_DICOM: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_DICOM);
    pub const APPLICATION_DICOM_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DICOM_JSON);
    pub const APPLICATION_DICOM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DICOM_XML);
    pub const APPLICATION_DNS: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_DNS);
    pub const APPLICATION_DNS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DNS_JSON);
    pub const APPLICATION_DNS_MESSAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DNS_MESSAGE);
    pub const APPLICATION_DOTS_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DOTS_CBOR);
    pub const APPLICATION_DPOP_JWT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DPOP_JWT);
    pub const APPLICATION_DSKPP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DSKPP_XML);
    pub const APPLICATION_DSSC_DER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DSSC_DER);
    pub const APPLICATION_DSSC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_DSSC_XML);
    pub const APPLICATION_DVCS: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_DVCS);
    pub const APPLICATION_ECMASCRIPT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ECMASCRIPT);
    pub const APPLICATION_EDHOC_CBOR_SEQ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EDHOC_CBOR_SEQ);
    pub const APPLICATION_EFI: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_EFI);
    pub const APPLICATION_ELM_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ELM_JSON);
    pub const APPLICATION_ELM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ELM_XML);
    pub const APPLICATION_EMMA_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMMA_XML);
    pub const APPLICATION_EMOTIONML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EMOTIONML_XML);
    pub const APPLICATION_ENCAPRTP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ENCAPRTP);
    pub const APPLICATION_EPP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EPP_XML);
    pub const APPLICATION_EPUB_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EPUB_ZIP);
    pub const APPLICATION_ESHOP: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_ESHOP);
    pub const APPLICATION_EXAMPLE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EXAMPLE);
    pub const APPLICATION_EXI: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_EXI);
    pub const APPLICATION_EXPECT_CT_REPORT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EXPECT_CT_REPORT_JSON);
    pub const APPLICATION_EXPRESS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_EXPRESS);
    pub const APPLICATION_FASTINFOSET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_FASTINFOSET);
    pub const APPLICATION_FASTSOAP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_FASTSOAP);
    pub const APPLICATION_FDF: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_FDF);
    pub const APPLICATION_FDT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_FDT_XML);
    pub const APPLICATION_FHIR_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_FHIR_JSON);
    pub const APPLICATION_FHIR_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_FHIR_XML);
    pub const APPLICATION_FITS: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_FITS);
    pub const APPLICATION_FLEXFEC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_FLEXFEC);
    pub const APPLICATION_FONT_SFNT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_FONT_SFNT);
    pub const APPLICATION_FONT_TDPFR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_FONT_TDPFR);
    pub const APPLICATION_FONT_WOFF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_FONT_WOFF);
    pub const APPLICATION_FRAMEWORK_ATTRIBUTES_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_FRAMEWORK_ATTRIBUTES_XML);
    pub const APPLICATION_GEO_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_GEO_JSON);
    pub const APPLICATION_GEO_JSON_SEQ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_GEO_JSON_SEQ);
    pub const APPLICATION_GEOPACKAGE_SQLITE3: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_GEOPACKAGE_SQLITE3);
    pub const APPLICATION_GEOXACML_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_GEOXACML_JSON);
    pub const APPLICATION_GEOXACML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_GEOXACML_XML);
    pub const APPLICATION_GLTF_BUFFER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_GLTF_BUFFER);
    pub const APPLICATION_GML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_GML_XML);
    pub const APPLICATION_GZIP: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_GZIP);
    pub const APPLICATION_HELD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_HELD_XML);
    pub const APPLICATION_HL7V2_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_HL7V2_XML);
    pub const APPLICATION_HTTP: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_HTTP);
    pub const APPLICATION_HYPERSTUDIO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_HYPERSTUDIO);
    pub const APPLICATION_IBE_KEY_REQUEST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_IBE_KEY_REQUEST_XML);
    pub const APPLICATION_IBE_PKG_REPLY_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_IBE_PKG_REPLY_XML);
    pub const APPLICATION_IBE_PP_DATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_IBE_PP_DATA);
    pub const APPLICATION_IGES: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_IGES);
    pub const APPLICATION_IM_ISCOMPOSING_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_IM_ISCOMPOSING_XML);
    pub const APPLICATION_INDEX: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_INDEX);
    pub const APPLICATION_INDEX_CMD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_INDEX_CMD);
    pub const APPLICATION_INDEX_OBJ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_INDEX_OBJ);
    pub const APPLICATION_INDEX_RESPONSE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_INDEX_RESPONSE);
    pub const APPLICATION_INDEX_VND: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_INDEX_VND);
    pub const APPLICATION_INKML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_INKML_XML);
    pub const APPLICATION_IPFIX: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_IPFIX);
    pub const APPLICATION_IPP: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_IPP);
    pub const APPLICATION_ITS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ITS_XML);
    pub const APPLICATION_JAVA_ARCHIVE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JAVA_ARCHIVE);
    pub const APPLICATION_JAVASCRIPT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JAVASCRIPT);
    pub const APPLICATION_JF2FEED_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JF2FEED_JSON);
    pub const APPLICATION_JOSE: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_JOSE);
    pub const APPLICATION_JOSE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JOSE_JSON);
    pub const APPLICATION_JRD_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JRD_JSON);
    pub const APPLICATION_JSCALENDAR_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JSCALENDAR_JSON);
    pub const APPLICATION_JSCONTACT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JSCONTACT_JSON);
    pub const APPLICATION_JSON: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_JSON);
    pub const APPLICATION_JSON_PATCH_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JSON_PATCH_JSON);
    pub const APPLICATION_JSON_SEQ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JSON_SEQ);
    pub const APPLICATION_JSONPATH: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JSONPATH);
    pub const APPLICATION_JWK_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JWK_JSON);
    pub const APPLICATION_JWK_SET_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_JWK_SET_JSON);
    pub const APPLICATION_JWT: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_JWT);
    pub const APPLICATION_KPML_REQUEST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_KPML_REQUEST_XML);
    pub const APPLICATION_KPML_RESPONSE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_KPML_RESPONSE_XML);
    pub const APPLICATION_LD_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_LD_JSON);
    pub const APPLICATION_LGR_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_LGR_XML);
    pub const APPLICATION_LINK_FORMAT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_LINK_FORMAT);
    pub const APPLICATION_LINKSET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_LINKSET);
    pub const APPLICATION_LINKSET_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_LINKSET_JSON);
    pub const APPLICATION_LOAD_CONTROL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_LOAD_CONTROL_XML);
    pub const APPLICATION_LOGOUT_JWT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_LOGOUT_JWT);
    pub const APPLICATION_LOST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_LOST_XML);
    pub const APPLICATION_LOSTSYNC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_LOSTSYNC_XML);
    pub const APPLICATION_LPF_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_LPF_ZIP);
    pub const APPLICATION_MAC_BINHEX40: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MAC_BINHEX40);
    pub const APPLICATION_MACWRITEII: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MACWRITEII);
    pub const APPLICATION_MADS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MADS_XML);
    pub const APPLICATION_MANIFEST_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MANIFEST_JSON);
    pub const APPLICATION_MARC: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_MARC);
    pub const APPLICATION_MARCXML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MARCXML_XML);
    pub const APPLICATION_MATHEMATICA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MATHEMATICA);
    pub const APPLICATION_MATHML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MATHML_XML);
    pub const APPLICATION_MATHML_CONTENT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MATHML_CONTENT_XML);
    pub const APPLICATION_MATHML_PRESENTATION_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MATHML_PRESENTATION_XML);
    pub const APPLICATION_MBMS_ASSOCIATED_PROCEDURE_DESCRIPTION_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MBMS_ASSOCIATED_PROCEDURE_DESCRIPTION_XML);
    pub const APPLICATION_MBMS_DEREGISTER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MBMS_DEREGISTER_XML);
    pub const APPLICATION_MBMS_ENVELOPE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MBMS_ENVELOPE_XML);
    pub const APPLICATION_MBMS_MSK_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MBMS_MSK_XML);
    pub const APPLICATION_MBMS_MSK_RESPONSE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MBMS_MSK_RESPONSE_XML);
    pub const APPLICATION_MBMS_PROTECTION_DESCRIPTION_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MBMS_PROTECTION_DESCRIPTION_XML);
    pub const APPLICATION_MBMS_RECEPTION_REPORT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MBMS_RECEPTION_REPORT_XML);
    pub const APPLICATION_MBMS_REGISTER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MBMS_REGISTER_XML);
    pub const APPLICATION_MBMS_REGISTER_RESPONSE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MBMS_REGISTER_RESPONSE_XML);
    pub const APPLICATION_MBMS_SCHEDULE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MBMS_SCHEDULE_XML);
    pub const APPLICATION_MBMS_USER_SERVICE_DESCRIPTION_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MBMS_USER_SERVICE_DESCRIPTION_XML);
    pub const APPLICATION_MBOX: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_MBOX);
    pub const APPLICATION_MEDIA_POLICY_DATASET_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MEDIA_POLICY_DATASET_XML);
    pub const APPLICATION_MEDIA_CONTROL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MEDIA_CONTROL_XML);
    pub const APPLICATION_MEDIASERVERCONTROL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MEDIASERVERCONTROL_XML);
    pub const APPLICATION_MERGE_PATCH_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MERGE_PATCH_JSON);
    pub const APPLICATION_METALINK4_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_METALINK4_XML);
    pub const APPLICATION_METS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_METS_XML);
    pub const APPLICATION_MIKEY: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_MIKEY);
    pub const APPLICATION_MIPC: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_MIPC);
    pub const APPLICATION_MISSING_BLOCKS_CBOR_SEQ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MISSING_BLOCKS_CBOR_SEQ);
    pub const APPLICATION_MMT_AEI_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MMT_AEI_XML);
    pub const APPLICATION_MMT_USD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MMT_USD_XML);
    pub const APPLICATION_MODS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MODS_XML);
    pub const APPLICATION_MOSS_KEYS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MOSS_KEYS);
    pub const APPLICATION_MOSS_SIGNATURE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MOSS_SIGNATURE);
    pub const APPLICATION_MOSSKEY_DATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MOSSKEY_DATA);
    pub const APPLICATION_MOSSKEY_REQUEST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MOSSKEY_REQUEST);
    pub const APPLICATION_MP21: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_MP21);
    pub const APPLICATION_MP4: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_MP4);
    pub const APPLICATION_MPEG4_GENERIC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MPEG4_GENERIC);
    pub const APPLICATION_MPEG4_IOD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MPEG4_IOD);
    pub const APPLICATION_MPEG4_IOD_XMT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MPEG4_IOD_XMT);
    pub const APPLICATION_MRB_CONSUMER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MRB_CONSUMER_XML);
    pub const APPLICATION_MRB_PUBLISH_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MRB_PUBLISH_XML);
    pub const APPLICATION_MSC_IVR_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MSC_IVR_XML);
    pub const APPLICATION_MSC_MIXER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MSC_MIXER_XML);
    pub const APPLICATION_MSWORD: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_MSWORD);
    pub const APPLICATION_MUD_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MUD_JSON);
    pub const APPLICATION_MULTIPART_CORE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_MULTIPART_CORE);
    pub const APPLICATION_MXF: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_MXF);
    pub const APPLICATION_N_QUADS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_N_QUADS);
    pub const APPLICATION_N_TRIPLES: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_N_TRIPLES);
    pub const APPLICATION_NASDATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_NASDATA);
    pub const APPLICATION_NEWS_CHECKGROUPS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_NEWS_CHECKGROUPS);
    pub const APPLICATION_NEWS_GROUPINFO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_NEWS_GROUPINFO);
    pub const APPLICATION_NEWS_TRANSMISSION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_NEWS_TRANSMISSION);
    pub const APPLICATION_NLSML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_NLSML_XML);
    pub const APPLICATION_NODE: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_NODE);
    pub const APPLICATION_NSS: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_NSS);
    pub const APPLICATION_OAUTH_AUTHZ_REQ_JWT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_OAUTH_AUTHZ_REQ_JWT);
    pub const APPLICATION_OBLIVIOUS_DNS_MESSAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_OBLIVIOUS_DNS_MESSAGE);
    pub const APPLICATION_OCSP_REQUEST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_OCSP_REQUEST);
    pub const APPLICATION_OCSP_RESPONSE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_OCSP_RESPONSE);
    pub const APPLICATION_OCTET_STREAM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_OCTET_STREAM);
    pub const APPLICATION_ODM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ODM_XML);
    pub const APPLICATION_OEBPS_PACKAGE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_OEBPS_PACKAGE_XML);
    pub const APPLICATION_OGG: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_OGG);
    pub const APPLICATION_OHTTP_KEYS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_OHTTP_KEYS);
    pub const APPLICATION_OPC_NODESET_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_OPC_NODESET_XML);
    pub const APPLICATION_OSCORE: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_OSCORE);
    pub const APPLICATION_OXPS: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_OXPS);
    pub const APPLICATION_P21: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_P21);
    pub const APPLICATION_P21_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_P21_ZIP);
    pub const APPLICATION_P2P_OVERLAY_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_P2P_OVERLAY_XML);
    pub const APPLICATION_PARITYFEC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PARITYFEC);
    pub const APPLICATION_PASSPORT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PASSPORT);
    pub const APPLICATION_PATCH_OPS_ERROR_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PATCH_OPS_ERROR_XML);
    pub const APPLICATION_PDF: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_PDF);
    pub const APPLICATION_PEM_CERTIFICATE_CHAIN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PEM_CERTIFICATE_CHAIN);
    pub const APPLICATION_PGP_ENCRYPTED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PGP_ENCRYPTED);
    pub const APPLICATION_PGP_KEYS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PGP_KEYS);
    pub const APPLICATION_PGP_SIGNATURE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PGP_SIGNATURE);
    pub const APPLICATION_PIDF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PIDF_XML);
    pub const APPLICATION_PIDF_DIFF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PIDF_DIFF_XML);
    pub const APPLICATION_PKCS10: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_PKCS10);
    pub const APPLICATION_PKCS12: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_PKCS12);
    pub const APPLICATION_PKCS7_MIME: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PKCS7_MIME);
    pub const APPLICATION_PKCS7_SIGNATURE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PKCS7_SIGNATURE);
    pub const APPLICATION_PKCS8: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_PKCS8);
    pub const APPLICATION_PKCS8_ENCRYPTED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PKCS8_ENCRYPTED);
    pub const APPLICATION_PKIX_ATTR_CERT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PKIX_ATTR_CERT);
    pub const APPLICATION_PKIX_CERT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PKIX_CERT);
    pub const APPLICATION_PKIX_CRL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PKIX_CRL);
    pub const APPLICATION_PKIX_PKIPATH: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PKIX_PKIPATH);
    pub const APPLICATION_PKIXCMP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PKIXCMP);
    pub const APPLICATION_PLS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PLS_XML);
    pub const APPLICATION_POC_SETTINGS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_POC_SETTINGS_XML);
    pub const APPLICATION_POSTSCRIPT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_POSTSCRIPT);
    pub const APPLICATION_PPSP_TRACKER_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PPSP_TRACKER_JSON);
    pub const APPLICATION_PRIVATE_TOKEN_ISSUER_DIRECTORY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRIVATE_TOKEN_ISSUER_DIRECTORY);
    pub const APPLICATION_PRIVATE_TOKEN_REQUEST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRIVATE_TOKEN_REQUEST);
    pub const APPLICATION_PRIVATE_TOKEN_RESPONSE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRIVATE_TOKEN_RESPONSE);
    pub const APPLICATION_PROBLEM_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PROBLEM_JSON);
    pub const APPLICATION_PROBLEM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PROBLEM_XML);
    pub const APPLICATION_PROVENANCE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PROVENANCE_XML);
    pub const APPLICATION_PRS_ALVESTRAND_TITRAX_SHEET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_ALVESTRAND_TITRAX_SHEET);
    pub const APPLICATION_PRS_CWW: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_CWW);
    pub const APPLICATION_PRS_CYN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_CYN);
    pub const APPLICATION_PRS_HPUB_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_HPUB_ZIP);
    pub const APPLICATION_PRS_IMPLIED_DOCUMENT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_IMPLIED_DOCUMENT_XML);
    pub const APPLICATION_PRS_IMPLIED_EXECUTABLE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_IMPLIED_EXECUTABLE);
    pub const APPLICATION_PRS_IMPLIED_OBJECT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_IMPLIED_OBJECT_JSON);
    pub const APPLICATION_PRS_IMPLIED_OBJECT_JSON_SEQ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_IMPLIED_OBJECT_JSON_SEQ);
    pub const APPLICATION_PRS_IMPLIED_OBJECT_YAML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_IMPLIED_OBJECT_YAML);
    pub const APPLICATION_PRS_IMPLIED_STRUCTURE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_IMPLIED_STRUCTURE);
    pub const APPLICATION_PRS_NPREND: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_NPREND);
    pub const APPLICATION_PRS_PLUCKER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_PLUCKER);
    pub const APPLICATION_PRS_RDF_XML_CRYPT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_RDF_XML_CRYPT);
    pub const APPLICATION_PRS_VCFBZIP2: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_VCFBZIP2);
    pub const APPLICATION_PRS_XSF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PRS_XSF_XML);
    pub const APPLICATION_PSKC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PSKC_XML);
    pub const APPLICATION_PVD_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_PVD_JSON);
    pub const APPLICATION_RAPTORFEC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RAPTORFEC);
    pub const APPLICATION_RDAP_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RDAP_JSON);
    pub const APPLICATION_RDF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RDF_XML);
    pub const APPLICATION_REGINFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_REGINFO_XML);
    pub const APPLICATION_RELAX_NG_COMPACT_SYNTAX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RELAX_NG_COMPACT_SYNTAX);
    pub const APPLICATION_REMOTE_PRINTING: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_REMOTE_PRINTING);
    pub const APPLICATION_REPUTON_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_REPUTON_JSON);
    pub const APPLICATION_RESOURCE_LISTS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RESOURCE_LISTS_XML);
    pub const APPLICATION_RESOURCE_LISTS_DIFF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RESOURCE_LISTS_DIFF_XML);
    pub const APPLICATION_RFC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RFC_XML);
    pub const APPLICATION_RISCOS: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_RISCOS);
    pub const APPLICATION_RLMI_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RLMI_XML);
    pub const APPLICATION_RLS_SERVICES_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RLS_SERVICES_XML);
    pub const APPLICATION_ROUTE_APD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ROUTE_APD_XML);
    pub const APPLICATION_ROUTE_S_TSID_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ROUTE_S_TSID_XML);
    pub const APPLICATION_ROUTE_USD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_ROUTE_USD_XML);
    pub const APPLICATION_RPKI_CHECKLIST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RPKI_CHECKLIST);
    pub const APPLICATION_RPKI_GHOSTBUSTERS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RPKI_GHOSTBUSTERS);
    pub const APPLICATION_RPKI_MANIFEST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RPKI_MANIFEST);
    pub const APPLICATION_RPKI_PUBLICATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RPKI_PUBLICATION);
    pub const APPLICATION_RPKI_ROA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RPKI_ROA);
    pub const APPLICATION_RPKI_UPDOWN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RPKI_UPDOWN);
    pub const APPLICATION_RTF: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_RTF);
    pub const APPLICATION_RTPLOOPBACK: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_RTPLOOPBACK);
    pub const APPLICATION_RTX: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_RTX);
    pub const APPLICATION_SAMLASSERTION_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SAMLASSERTION_XML);
    pub const APPLICATION_SAMLMETADATA_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SAMLMETADATA_XML);
    pub const APPLICATION_SARIF_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SARIF_JSON);
    pub const APPLICATION_SARIF_EXTERNAL_PROPERTIES_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SARIF_EXTERNAL_PROPERTIES_JSON);
    pub const APPLICATION_SBE: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_SBE);
    pub const APPLICATION_SBML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SBML_XML);
    pub const APPLICATION_SCAIP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SCAIP_XML);
    pub const APPLICATION_SCIM_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SCIM_JSON);
    pub const APPLICATION_SCVP_CV_REQUEST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SCVP_CV_REQUEST);
    pub const APPLICATION_SCVP_CV_RESPONSE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SCVP_CV_RESPONSE);
    pub const APPLICATION_SCVP_VP_REQUEST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SCVP_VP_REQUEST);
    pub const APPLICATION_SCVP_VP_RESPONSE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SCVP_VP_RESPONSE);
    pub const APPLICATION_SDP: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_SDP);
    pub const APPLICATION_SECEVENT_JWT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SECEVENT_JWT);
    pub const APPLICATION_SENML_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SENML_CBOR);
    pub const APPLICATION_SENML_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SENML_JSON);
    pub const APPLICATION_SENML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SENML_XML);
    pub const APPLICATION_SENML_ETCH_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SENML_ETCH_CBOR);
    pub const APPLICATION_SENML_ETCH_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SENML_ETCH_JSON);
    pub const APPLICATION_SENML_EXI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SENML_EXI);
    pub const APPLICATION_SENSML_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SENSML_CBOR);
    pub const APPLICATION_SENSML_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SENSML_JSON);
    pub const APPLICATION_SENSML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SENSML_XML);
    pub const APPLICATION_SENSML_EXI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SENSML_EXI);
    pub const APPLICATION_SEP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SEP_XML);
    pub const APPLICATION_SEP_EXI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SEP_EXI);
    pub const APPLICATION_SESSION_INFO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SESSION_INFO);
    pub const APPLICATION_SET_PAYMENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SET_PAYMENT);
    pub const APPLICATION_SET_PAYMENT_INITIATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SET_PAYMENT_INITIATION);
    pub const APPLICATION_SET_REGISTRATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SET_REGISTRATION);
    pub const APPLICATION_SET_REGISTRATION_INITIATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SET_REGISTRATION_INITIATION);
    pub const APPLICATION_SGML_OPEN_CATALOG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SGML_OPEN_CATALOG);
    pub const APPLICATION_SHF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SHF_XML);
    pub const APPLICATION_SIEVE: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_SIEVE);
    pub const APPLICATION_SIMPLE_FILTER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SIMPLE_FILTER_XML);
    pub const APPLICATION_SIMPLE_MESSAGE_SUMMARY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SIMPLE_MESSAGE_SUMMARY);
    pub const APPLICATION_SIMPLESYMBOLCONTAINER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SIMPLESYMBOLCONTAINER);
    pub const APPLICATION_SIPC: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_SIPC);
    pub const APPLICATION_SLATE: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_SLATE);
    pub const APPLICATION_SMIL: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_SMIL);
    pub const APPLICATION_SMIL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SMIL_XML);
    pub const APPLICATION_SMPTE336M: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SMPTE336M);
    pub const APPLICATION_SOAP_FASTINFOSET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SOAP_FASTINFOSET);
    pub const APPLICATION_SOAP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SOAP_XML);
    pub const APPLICATION_SPARQL_QUERY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SPARQL_QUERY);
    pub const APPLICATION_SPARQL_RESULTS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SPARQL_RESULTS_XML);
    pub const APPLICATION_SPDX_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SPDX_JSON);
    pub const APPLICATION_SPIRITS_EVENT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SPIRITS_EVENT_XML);
    pub const APPLICATION_SQL: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_SQL);
    pub const APPLICATION_SRGS: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_SRGS);
    pub const APPLICATION_SRGS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SRGS_XML);
    pub const APPLICATION_SRU_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SRU_XML);
    pub const APPLICATION_SSML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SSML_XML);
    pub const APPLICATION_STIX_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_STIX_JSON);
    pub const APPLICATION_SWID_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SWID_CBOR);
    pub const APPLICATION_SWID_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_SWID_XML);
    pub const APPLICATION_TAMP_APEX_UPDATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAMP_APEX_UPDATE);
    pub const APPLICATION_TAMP_APEX_UPDATE_CONFIRM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAMP_APEX_UPDATE_CONFIRM);
    pub const APPLICATION_TAMP_COMMUNITY_UPDATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAMP_COMMUNITY_UPDATE);
    pub const APPLICATION_TAMP_COMMUNITY_UPDATE_CONFIRM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAMP_COMMUNITY_UPDATE_CONFIRM);
    pub const APPLICATION_TAMP_ERROR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAMP_ERROR);
    pub const APPLICATION_TAMP_SEQUENCE_ADJUST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAMP_SEQUENCE_ADJUST);
    pub const APPLICATION_TAMP_SEQUENCE_ADJUST_CONFIRM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAMP_SEQUENCE_ADJUST_CONFIRM);
    pub const APPLICATION_TAMP_STATUS_QUERY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAMP_STATUS_QUERY);
    pub const APPLICATION_TAMP_STATUS_RESPONSE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAMP_STATUS_RESPONSE);
    pub const APPLICATION_TAMP_UPDATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAMP_UPDATE);
    pub const APPLICATION_TAMP_UPDATE_CONFIRM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAMP_UPDATE_CONFIRM);
    pub const APPLICATION_TAXII_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TAXII_JSON);
    pub const APPLICATION_TD_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TD_JSON);
    pub const APPLICATION_TEI_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TEI_XML);
    pub const APPLICATION_THRAUD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_THRAUD_XML);
    pub const APPLICATION_TIMESTAMP_QUERY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TIMESTAMP_QUERY);
    pub const APPLICATION_TIMESTAMP_REPLY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TIMESTAMP_REPLY);
    pub const APPLICATION_TIMESTAMPED_DATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TIMESTAMPED_DATA);
    pub const APPLICATION_TLSRPT_GZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TLSRPT_GZIP);
    pub const APPLICATION_TLSRPT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TLSRPT_JSON);
    pub const APPLICATION_TM_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TM_JSON);
    pub const APPLICATION_TNAUTHLIST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TNAUTHLIST);
    pub const APPLICATION_TOKEN_INTROSPECTION_JWT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TOKEN_INTROSPECTION_JWT);
    pub const APPLICATION_TRICKLE_ICE_SDPFRAG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TRICKLE_ICE_SDPFRAG);
    pub const APPLICATION_TRIG: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_TRIG);
    pub const APPLICATION_TTML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TTML_XML);
    pub const APPLICATION_TVE_TRIGGER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TVE_TRIGGER);
    pub const APPLICATION_TZIF: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_TZIF);
    pub const APPLICATION_TZIF_LEAP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_TZIF_LEAP);
    pub const APPLICATION_ULPFEC: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_ULPFEC);
    pub const APPLICATION_URC_GRPSHEET_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_URC_GRPSHEET_XML);
    pub const APPLICATION_URC_RESSHEET_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_URC_RESSHEET_XML);
    pub const APPLICATION_URC_TARGETDESC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_URC_TARGETDESC_XML);
    pub const APPLICATION_URC_UISOCKETDESC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_URC_UISOCKETDESC_XML);
    pub const APPLICATION_VCARD_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VCARD_JSON);
    pub const APPLICATION_VCARD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VCARD_XML);
    pub const APPLICATION_VEMMI: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VEMMI);
    pub const APPLICATION_VND_1000MINDS_DECISION_MODEL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_1000MINDS_DECISION_MODEL_XML);
    pub const APPLICATION_VND_1OB: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_1OB);
    pub const APPLICATION_VND_3M_POST_IT_NOTES: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3M_POST_IT_NOTES);
    pub const APPLICATION_VND_3GPP_PROSE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_PROSE_XML);
    pub const APPLICATION_VND_3GPP_PROSE_PC3A_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_PROSE_PC3A_XML);
    pub const APPLICATION_VND_3GPP_PROSE_PC3ACH_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_PROSE_PC3ACH_XML);
    pub const APPLICATION_VND_3GPP_PROSE_PC3CH_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_PROSE_PC3CH_XML);
    pub const APPLICATION_VND_3GPP_PROSE_PC8_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_PROSE_PC8_XML);
    pub const APPLICATION_VND_3GPP_V2X_LOCAL_SERVICE_INFORMATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_V2X_LOCAL_SERVICE_INFORMATION);
    pub const APPLICATION_VND_3GPP_5GNAS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_5GNAS);
    pub const APPLICATION_VND_3GPP_GMOP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_GMOP_XML);
    pub const APPLICATION_VND_3GPP_SRVCC_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_SRVCC_INFO_XML);
    pub const APPLICATION_VND_3GPP_ACCESS_TRANSFER_EVENTS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_ACCESS_TRANSFER_EVENTS_XML);
    pub const APPLICATION_VND_3GPP_BSF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_BSF_XML);
    pub const APPLICATION_VND_3GPP_CRS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_CRS_XML);
    pub const APPLICATION_VND_3GPP_CURRENT_LOCATION_DISCOVERY_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_CURRENT_LOCATION_DISCOVERY_XML);
    pub const APPLICATION_VND_3GPP_GTPC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_GTPC);
    pub const APPLICATION_VND_3GPP_INTERWORKING_DATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_INTERWORKING_DATA);
    pub const APPLICATION_VND_3GPP_LPP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_LPP);
    pub const APPLICATION_VND_3GPP_MC_SIGNALLING_EAR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MC_SIGNALLING_EAR);
    pub const APPLICATION_VND_3GPP_MCDATA_AFFILIATION_COMMAND_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCDATA_AFFILIATION_COMMAND_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCDATA_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_MSGSTORE_CTRL_REQUEST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCDATA_MSGSTORE_CTRL_REQUEST_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_PAYLOAD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCDATA_PAYLOAD);
    pub const APPLICATION_VND_3GPP_MCDATA_REGROUP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCDATA_REGROUP_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_SERVICE_CONFIG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCDATA_SERVICE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_SIGNALLING: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCDATA_SIGNALLING);
    pub const APPLICATION_VND_3GPP_MCDATA_UE_CONFIG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCDATA_UE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCDATA_USER_PROFILE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCDATA_USER_PROFILE_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_AFFILIATION_COMMAND_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCPTT_AFFILIATION_COMMAND_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_FLOOR_REQUEST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCPTT_FLOOR_REQUEST_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCPTT_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_LOCATION_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCPTT_LOCATION_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_MBMS_USAGE_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCPTT_MBMS_USAGE_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_REGROUP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCPTT_REGROUP_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_SERVICE_CONFIG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCPTT_SERVICE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_SIGNED_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCPTT_SIGNED_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_UE_CONFIG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCPTT_UE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_UE_INIT_CONFIG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCPTT_UE_INIT_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCPTT_USER_PROFILE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCPTT_USER_PROFILE_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_COMMAND_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_COMMAND_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCVIDEO_AFFILIATION_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCVIDEO_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_LOCATION_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCVIDEO_LOCATION_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_MBMS_USAGE_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCVIDEO_MBMS_USAGE_INFO_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_REGROUP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCVIDEO_REGROUP_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_SERVICE_CONFIG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCVIDEO_SERVICE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_TRANSMISSION_REQUEST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCVIDEO_TRANSMISSION_REQUEST_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_UE_CONFIG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCVIDEO_UE_CONFIG_XML);
    pub const APPLICATION_VND_3GPP_MCVIDEO_USER_PROFILE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MCVIDEO_USER_PROFILE_XML);
    pub const APPLICATION_VND_3GPP_MID_CALL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_MID_CALL_XML);
    pub const APPLICATION_VND_3GPP_NGAP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_NGAP);
    pub const APPLICATION_VND_3GPP_PFCP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_PFCP);
    pub const APPLICATION_VND_3GPP_PIC_BW_LARGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_PIC_BW_LARGE);
    pub const APPLICATION_VND_3GPP_PIC_BW_SMALL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_PIC_BW_SMALL);
    pub const APPLICATION_VND_3GPP_PIC_BW_VAR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_PIC_BW_VAR);
    pub const APPLICATION_VND_3GPP_S1AP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_S1AP);
    pub const APPLICATION_VND_3GPP_SEAL_GROUP_DOC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_SEAL_GROUP_DOC_XML);
    pub const APPLICATION_VND_3GPP_SEAL_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_SEAL_INFO_XML);
    pub const APPLICATION_VND_3GPP_SEAL_LOCATION_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_SEAL_LOCATION_INFO_XML);
    pub const APPLICATION_VND_3GPP_SEAL_MBMS_USAGE_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_SEAL_MBMS_USAGE_INFO_XML);
    pub const APPLICATION_VND_3GPP_SEAL_NETWORK_QOS_MANAGEMENT_INFO_XML: Encoding = Encoding::new(
        IanaEncodingMapping::APPLICATION_VND_3GPP_SEAL_NETWORK_QOS_MANAGEMENT_INFO_XML,
    );
    pub const APPLICATION_VND_3GPP_SEAL_UE_CONFIG_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_SEAL_UE_CONFIG_INFO_XML);
    pub const APPLICATION_VND_3GPP_SEAL_UNICAST_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_SEAL_UNICAST_INFO_XML);
    pub const APPLICATION_VND_3GPP_SEAL_USER_PROFILE_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_SEAL_USER_PROFILE_INFO_XML);
    pub const APPLICATION_VND_3GPP_SMS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_SMS);
    pub const APPLICATION_VND_3GPP_SMS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_SMS_XML);
    pub const APPLICATION_VND_3GPP_SRVCC_EXT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_SRVCC_EXT_XML);
    pub const APPLICATION_VND_3GPP_STATE_AND_EVENT_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_STATE_AND_EVENT_INFO_XML);
    pub const APPLICATION_VND_3GPP_USSD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_USSD_XML);
    pub const APPLICATION_VND_3GPP_V2X: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_V2X);
    pub const APPLICATION_VND_3GPP_VAE_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP_VAE_INFO_XML);
    pub const APPLICATION_VND_3GPP2_BCMCSINFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP2_BCMCSINFO_XML);
    pub const APPLICATION_VND_3GPP2_SMS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP2_SMS);
    pub const APPLICATION_VND_3GPP2_TCAP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3GPP2_TCAP);
    pub const APPLICATION_VND_3LIGHTSSOFTWARE_IMAGESCAL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_3LIGHTSSOFTWARE_IMAGESCAL);
    pub const APPLICATION_VND_FLOGRAPHIT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FLOGRAPHIT);
    pub const APPLICATION_VND_HANDHELD_ENTERTAINMENT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HANDHELD_ENTERTAINMENT_XML);
    pub const APPLICATION_VND_KINAR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KINAR);
    pub const APPLICATION_VND_MFER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MFER);
    pub const APPLICATION_VND_MOBIUS_DAF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOBIUS_DAF);
    pub const APPLICATION_VND_MOBIUS_DIS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOBIUS_DIS);
    pub const APPLICATION_VND_MOBIUS_MBK: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOBIUS_MBK);
    pub const APPLICATION_VND_MOBIUS_MQY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOBIUS_MQY);
    pub const APPLICATION_VND_MOBIUS_MSL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOBIUS_MSL);
    pub const APPLICATION_VND_MOBIUS_PLC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOBIUS_PLC);
    pub const APPLICATION_VND_MOBIUS_TXF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOBIUS_TXF);
    pub const APPLICATION_VND_QUARK_QUARKXPRESS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_QUARK_QUARKXPRESS);
    pub const APPLICATION_VND_RENLEARN_RLPRINT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RENLEARN_RLPRINT);
    pub const APPLICATION_VND_SIMTECH_MINDMAPPER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SIMTECH_MINDMAPPER);
    pub const APPLICATION_VND_ACCPAC_SIMPLY_ASO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ACCPAC_SIMPLY_ASO);
    pub const APPLICATION_VND_ACCPAC_SIMPLY_IMP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ACCPAC_SIMPLY_IMP);
    pub const APPLICATION_VND_ACM_ADDRESSXFER_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ACM_ADDRESSXFER_JSON);
    pub const APPLICATION_VND_ACM_CHATBOT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ACM_CHATBOT_JSON);
    pub const APPLICATION_VND_ACUCOBOL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ACUCOBOL);
    pub const APPLICATION_VND_ACUCORP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ACUCORP);
    pub const APPLICATION_VND_ADOBE_FLASH_MOVIE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ADOBE_FLASH_MOVIE);
    pub const APPLICATION_VND_ADOBE_FORMSCENTRAL_FCDT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ADOBE_FORMSCENTRAL_FCDT);
    pub const APPLICATION_VND_ADOBE_FXP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ADOBE_FXP);
    pub const APPLICATION_VND_ADOBE_PARTIAL_UPLOAD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ADOBE_PARTIAL_UPLOAD);
    pub const APPLICATION_VND_ADOBE_XDP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ADOBE_XDP_XML);
    pub const APPLICATION_VND_AETHER_IMP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AETHER_IMP);
    pub const APPLICATION_VND_AFPC_AFPLINEDATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_AFPLINEDATA);
    pub const APPLICATION_VND_AFPC_AFPLINEDATA_PAGEDEF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_AFPLINEDATA_PAGEDEF);
    pub const APPLICATION_VND_AFPC_CMOCA_CMRESOURCE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_CMOCA_CMRESOURCE);
    pub const APPLICATION_VND_AFPC_FOCA_CHARSET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_FOCA_CHARSET);
    pub const APPLICATION_VND_AFPC_FOCA_CODEDFONT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_FOCA_CODEDFONT);
    pub const APPLICATION_VND_AFPC_FOCA_CODEPAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_FOCA_CODEPAGE);
    pub const APPLICATION_VND_AFPC_MODCA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_MODCA);
    pub const APPLICATION_VND_AFPC_MODCA_CMTABLE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_MODCA_CMTABLE);
    pub const APPLICATION_VND_AFPC_MODCA_FORMDEF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_MODCA_FORMDEF);
    pub const APPLICATION_VND_AFPC_MODCA_MEDIUMMAP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_MODCA_MEDIUMMAP);
    pub const APPLICATION_VND_AFPC_MODCA_OBJECTCONTAINER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_MODCA_OBJECTCONTAINER);
    pub const APPLICATION_VND_AFPC_MODCA_OVERLAY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_MODCA_OVERLAY);
    pub const APPLICATION_VND_AFPC_MODCA_PAGESEGMENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AFPC_MODCA_PAGESEGMENT);
    pub const APPLICATION_VND_AGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AGE);
    pub const APPLICATION_VND_AH_BARCODE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AH_BARCODE);
    pub const APPLICATION_VND_AHEAD_SPACE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AHEAD_SPACE);
    pub const APPLICATION_VND_AIRZIP_FILESECURE_AZF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AIRZIP_FILESECURE_AZF);
    pub const APPLICATION_VND_AIRZIP_FILESECURE_AZS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AIRZIP_FILESECURE_AZS);
    pub const APPLICATION_VND_AMADEUS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AMADEUS_JSON);
    pub const APPLICATION_VND_AMAZON_MOBI8_EBOOK: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AMAZON_MOBI8_EBOOK);
    pub const APPLICATION_VND_AMERICANDYNAMICS_ACC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AMERICANDYNAMICS_ACC);
    pub const APPLICATION_VND_AMIGA_AMI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AMIGA_AMI);
    pub const APPLICATION_VND_AMUNDSEN_MAZE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AMUNDSEN_MAZE_XML);
    pub const APPLICATION_VND_ANDROID_OTA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ANDROID_OTA);
    pub const APPLICATION_VND_ANKI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ANKI);
    pub const APPLICATION_VND_ANSER_WEB_CERTIFICATE_ISSUE_INITIATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ANSER_WEB_CERTIFICATE_ISSUE_INITIATION);
    pub const APPLICATION_VND_ANTIX_GAME_COMPONENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ANTIX_GAME_COMPONENT);
    pub const APPLICATION_VND_APACHE_ARROW_FILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APACHE_ARROW_FILE);
    pub const APPLICATION_VND_APACHE_ARROW_STREAM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APACHE_ARROW_STREAM);
    pub const APPLICATION_VND_APACHE_PARQUET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APACHE_PARQUET);
    pub const APPLICATION_VND_APACHE_THRIFT_BINARY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APACHE_THRIFT_BINARY);
    pub const APPLICATION_VND_APACHE_THRIFT_COMPACT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APACHE_THRIFT_COMPACT);
    pub const APPLICATION_VND_APACHE_THRIFT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APACHE_THRIFT_JSON);
    pub const APPLICATION_VND_APEXLANG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APEXLANG);
    pub const APPLICATION_VND_API_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_API_JSON);
    pub const APPLICATION_VND_APLEXTOR_WARRP_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APLEXTOR_WARRP_JSON);
    pub const APPLICATION_VND_APOTHEKENDE_RESERVATION_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APOTHEKENDE_RESERVATION_JSON);
    pub const APPLICATION_VND_APPLE_INSTALLER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APPLE_INSTALLER_XML);
    pub const APPLICATION_VND_APPLE_KEYNOTE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APPLE_KEYNOTE);
    pub const APPLICATION_VND_APPLE_MPEGURL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APPLE_MPEGURL);
    pub const APPLICATION_VND_APPLE_NUMBERS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APPLE_NUMBERS);
    pub const APPLICATION_VND_APPLE_PAGES: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_APPLE_PAGES);
    pub const APPLICATION_VND_ARASTRA_SWI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ARASTRA_SWI);
    pub const APPLICATION_VND_ARISTANETWORKS_SWI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ARISTANETWORKS_SWI);
    pub const APPLICATION_VND_ARTISAN_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ARTISAN_JSON);
    pub const APPLICATION_VND_ARTSQUARE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ARTSQUARE);
    pub const APPLICATION_VND_ASTRAEA_SOFTWARE_IOTA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ASTRAEA_SOFTWARE_IOTA);
    pub const APPLICATION_VND_AUDIOGRAPH: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AUDIOGRAPH);
    pub const APPLICATION_VND_AUTOPACKAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AUTOPACKAGE);
    pub const APPLICATION_VND_AVALON_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AVALON_JSON);
    pub const APPLICATION_VND_AVISTAR_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_AVISTAR_XML);
    pub const APPLICATION_VND_BALSAMIQ_BMML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BALSAMIQ_BMML_XML);
    pub const APPLICATION_VND_BALSAMIQ_BMPR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BALSAMIQ_BMPR);
    pub const APPLICATION_VND_BANANA_ACCOUNTING: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BANANA_ACCOUNTING);
    pub const APPLICATION_VND_BBF_USP_ERROR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BBF_USP_ERROR);
    pub const APPLICATION_VND_BBF_USP_MSG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BBF_USP_MSG);
    pub const APPLICATION_VND_BBF_USP_MSG_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BBF_USP_MSG_JSON);
    pub const APPLICATION_VND_BEKITZUR_STECH_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BEKITZUR_STECH_JSON);
    pub const APPLICATION_VND_BELIGHTSOFT_LHZD_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BELIGHTSOFT_LHZD_ZIP);
    pub const APPLICATION_VND_BELIGHTSOFT_LHZL_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BELIGHTSOFT_LHZL_ZIP);
    pub const APPLICATION_VND_BINT_MED_CONTENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BINT_MED_CONTENT);
    pub const APPLICATION_VND_BIOPAX_RDF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BIOPAX_RDF_XML);
    pub const APPLICATION_VND_BLINK_IDB_VALUE_WRAPPER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BLINK_IDB_VALUE_WRAPPER);
    pub const APPLICATION_VND_BLUEICE_MULTIPASS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BLUEICE_MULTIPASS);
    pub const APPLICATION_VND_BLUETOOTH_EP_OOB: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BLUETOOTH_EP_OOB);
    pub const APPLICATION_VND_BLUETOOTH_LE_OOB: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BLUETOOTH_LE_OOB);
    pub const APPLICATION_VND_BMI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BMI);
    pub const APPLICATION_VND_BPF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BPF);
    pub const APPLICATION_VND_BPF3: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BPF3);
    pub const APPLICATION_VND_BUSINESSOBJECTS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BUSINESSOBJECTS);
    pub const APPLICATION_VND_BYU_UAPI_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BYU_UAPI_JSON);
    pub const APPLICATION_VND_BZIP3: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_BZIP3);
    pub const APPLICATION_VND_CAB_JSCRIPT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CAB_JSCRIPT);
    pub const APPLICATION_VND_CANON_CPDL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CANON_CPDL);
    pub const APPLICATION_VND_CANON_LIPS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CANON_LIPS);
    pub const APPLICATION_VND_CAPASYSTEMS_PG_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CAPASYSTEMS_PG_JSON);
    pub const APPLICATION_VND_CENDIO_THINLINC_CLIENTCONF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CENDIO_THINLINC_CLIENTCONF);
    pub const APPLICATION_VND_CENTURY_SYSTEMS_TCP_STREAM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CENTURY_SYSTEMS_TCP_STREAM);
    pub const APPLICATION_VND_CHEMDRAW_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CHEMDRAW_XML);
    pub const APPLICATION_VND_CHESS_PGN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CHESS_PGN);
    pub const APPLICATION_VND_CHIPNUTS_KARAOKE_MMD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CHIPNUTS_KARAOKE_MMD);
    pub const APPLICATION_VND_CIEDI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CIEDI);
    pub const APPLICATION_VND_CINDERELLA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CINDERELLA);
    pub const APPLICATION_VND_CIRPACK_ISDN_EXT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CIRPACK_ISDN_EXT);
    pub const APPLICATION_VND_CITATIONSTYLES_STYLE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CITATIONSTYLES_STYLE_XML);
    pub const APPLICATION_VND_CLAYMORE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CLAYMORE);
    pub const APPLICATION_VND_CLOANTO_RP9: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CLOANTO_RP9);
    pub const APPLICATION_VND_CLONK_C4GROUP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CLONK_C4GROUP);
    pub const APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG);
    pub const APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG_PKG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CLUETRUST_CARTOMOBILE_CONFIG_PKG);
    pub const APPLICATION_VND_CNCF_HELM_CHART_CONTENT_V1_TAR_GZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CNCF_HELM_CHART_CONTENT_V1_TAR_GZIP);
    pub const APPLICATION_VND_CNCF_HELM_CHART_PROVENANCE_V1_PROV: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CNCF_HELM_CHART_PROVENANCE_V1_PROV);
    pub const APPLICATION_VND_CNCF_HELM_CONFIG_V1_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CNCF_HELM_CONFIG_V1_JSON);
    pub const APPLICATION_VND_COFFEESCRIPT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COFFEESCRIPT);
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT);
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT_TEMPLATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COLLABIO_XODOCUMENTS_DOCUMENT_TEMPLATE);
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION);
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION_TEMPLATE: Encoding = Encoding::new(
        IanaEncodingMapping::APPLICATION_VND_COLLABIO_XODOCUMENTS_PRESENTATION_TEMPLATE,
    );
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET);
    pub const APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET_TEMPLATE: Encoding = Encoding::new(
        IanaEncodingMapping::APPLICATION_VND_COLLABIO_XODOCUMENTS_SPREADSHEET_TEMPLATE,
    );
    pub const APPLICATION_VND_COLLECTION_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COLLECTION_JSON);
    pub const APPLICATION_VND_COLLECTION_DOC_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COLLECTION_DOC_JSON);
    pub const APPLICATION_VND_COLLECTION_NEXT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COLLECTION_NEXT_JSON);
    pub const APPLICATION_VND_COMICBOOK_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COMICBOOK_ZIP);
    pub const APPLICATION_VND_COMICBOOK_RAR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COMICBOOK_RAR);
    pub const APPLICATION_VND_COMMERCE_BATTELLE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COMMERCE_BATTELLE);
    pub const APPLICATION_VND_COMMONSPACE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COMMONSPACE);
    pub const APPLICATION_VND_CONTACT_CMSG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CONTACT_CMSG);
    pub const APPLICATION_VND_COREOS_IGNITION_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COREOS_IGNITION_JSON);
    pub const APPLICATION_VND_COSMOCALLER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_COSMOCALLER);
    pub const APPLICATION_VND_CRICK_CLICKER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CRICK_CLICKER);
    pub const APPLICATION_VND_CRICK_CLICKER_KEYBOARD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CRICK_CLICKER_KEYBOARD);
    pub const APPLICATION_VND_CRICK_CLICKER_PALETTE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CRICK_CLICKER_PALETTE);
    pub const APPLICATION_VND_CRICK_CLICKER_TEMPLATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CRICK_CLICKER_TEMPLATE);
    pub const APPLICATION_VND_CRICK_CLICKER_WORDBANK: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CRICK_CLICKER_WORDBANK);
    pub const APPLICATION_VND_CRITICALTOOLS_WBS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CRITICALTOOLS_WBS_XML);
    pub const APPLICATION_VND_CRYPTII_PIPE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CRYPTII_PIPE_JSON);
    pub const APPLICATION_VND_CRYPTO_SHADE_FILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CRYPTO_SHADE_FILE);
    pub const APPLICATION_VND_CRYPTOMATOR_ENCRYPTED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CRYPTOMATOR_ENCRYPTED);
    pub const APPLICATION_VND_CRYPTOMATOR_VAULT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CRYPTOMATOR_VAULT);
    pub const APPLICATION_VND_CTC_POSML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CTC_POSML);
    pub const APPLICATION_VND_CTCT_WS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CTCT_WS_XML);
    pub const APPLICATION_VND_CUPS_PDF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CUPS_PDF);
    pub const APPLICATION_VND_CUPS_POSTSCRIPT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CUPS_POSTSCRIPT);
    pub const APPLICATION_VND_CUPS_PPD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CUPS_PPD);
    pub const APPLICATION_VND_CUPS_RASTER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CUPS_RASTER);
    pub const APPLICATION_VND_CUPS_RAW: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CUPS_RAW);
    pub const APPLICATION_VND_CURL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CURL);
    pub const APPLICATION_VND_CYAN_DEAN_ROOT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CYAN_DEAN_ROOT_XML);
    pub const APPLICATION_VND_CYBANK: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CYBANK);
    pub const APPLICATION_VND_CYCLONEDX_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CYCLONEDX_JSON);
    pub const APPLICATION_VND_CYCLONEDX_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_CYCLONEDX_XML);
    pub const APPLICATION_VND_D2L_COURSEPACKAGE1P0_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_D2L_COURSEPACKAGE1P0_ZIP);
    pub const APPLICATION_VND_D3M_DATASET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_D3M_DATASET);
    pub const APPLICATION_VND_D3M_PROBLEM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_D3M_PROBLEM);
    pub const APPLICATION_VND_DART: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DART);
    pub const APPLICATION_VND_DATA_VISION_RDZ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DATA_VISION_RDZ);
    pub const APPLICATION_VND_DATALOG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DATALOG);
    pub const APPLICATION_VND_DATAPACKAGE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DATAPACKAGE_JSON);
    pub const APPLICATION_VND_DATARESOURCE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DATARESOURCE_JSON);
    pub const APPLICATION_VND_DBF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DBF);
    pub const APPLICATION_VND_DEBIAN_BINARY_PACKAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DEBIAN_BINARY_PACKAGE);
    pub const APPLICATION_VND_DECE_DATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DECE_DATA);
    pub const APPLICATION_VND_DECE_TTML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DECE_TTML_XML);
    pub const APPLICATION_VND_DECE_UNSPECIFIED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DECE_UNSPECIFIED);
    pub const APPLICATION_VND_DECE_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DECE_ZIP);
    pub const APPLICATION_VND_DENOVO_FCSELAYOUT_LINK: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DENOVO_FCSELAYOUT_LINK);
    pub const APPLICATION_VND_DESMUME_MOVIE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DESMUME_MOVIE);
    pub const APPLICATION_VND_DIR_BI_PLATE_DL_NOSUFFIX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DIR_BI_PLATE_DL_NOSUFFIX);
    pub const APPLICATION_VND_DM_DELEGATION_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DM_DELEGATION_XML);
    pub const APPLICATION_VND_DNA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DNA);
    pub const APPLICATION_VND_DOCUMENT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DOCUMENT_JSON);
    pub const APPLICATION_VND_DOLBY_MOBILE_1: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DOLBY_MOBILE_1);
    pub const APPLICATION_VND_DOLBY_MOBILE_2: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DOLBY_MOBILE_2);
    pub const APPLICATION_VND_DOREMIR_SCORECLOUD_BINARY_DOCUMENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DOREMIR_SCORECLOUD_BINARY_DOCUMENT);
    pub const APPLICATION_VND_DPGRAPH: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DPGRAPH);
    pub const APPLICATION_VND_DREAMFACTORY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DREAMFACTORY);
    pub const APPLICATION_VND_DRIVE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DRIVE_JSON);
    pub const APPLICATION_VND_DTG_LOCAL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DTG_LOCAL);
    pub const APPLICATION_VND_DTG_LOCAL_FLASH: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DTG_LOCAL_FLASH);
    pub const APPLICATION_VND_DTG_LOCAL_HTML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DTG_LOCAL_HTML);
    pub const APPLICATION_VND_DVB_AIT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_AIT);
    pub const APPLICATION_VND_DVB_DVBISL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_DVBISL_XML);
    pub const APPLICATION_VND_DVB_DVBJ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_DVBJ);
    pub const APPLICATION_VND_DVB_ESGCONTAINER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_ESGCONTAINER);
    pub const APPLICATION_VND_DVB_IPDCDFTNOTIFACCESS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_IPDCDFTNOTIFACCESS);
    pub const APPLICATION_VND_DVB_IPDCESGACCESS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_IPDCESGACCESS);
    pub const APPLICATION_VND_DVB_IPDCESGACCESS2: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_IPDCESGACCESS2);
    pub const APPLICATION_VND_DVB_IPDCESGPDD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_IPDCESGPDD);
    pub const APPLICATION_VND_DVB_IPDCROAMING: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_IPDCROAMING);
    pub const APPLICATION_VND_DVB_IPTV_ALFEC_BASE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_IPTV_ALFEC_BASE);
    pub const APPLICATION_VND_DVB_IPTV_ALFEC_ENHANCEMENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_IPTV_ALFEC_ENHANCEMENT);
    pub const APPLICATION_VND_DVB_NOTIF_AGGREGATE_ROOT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_NOTIF_AGGREGATE_ROOT_XML);
    pub const APPLICATION_VND_DVB_NOTIF_CONTAINER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_NOTIF_CONTAINER_XML);
    pub const APPLICATION_VND_DVB_NOTIF_GENERIC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_NOTIF_GENERIC_XML);
    pub const APPLICATION_VND_DVB_NOTIF_IA_MSGLIST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_NOTIF_IA_MSGLIST_XML);
    pub const APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_REQUEST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_REQUEST_XML);
    pub const APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_RESPONSE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_NOTIF_IA_REGISTRATION_RESPONSE_XML);
    pub const APPLICATION_VND_DVB_NOTIF_INIT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_NOTIF_INIT_XML);
    pub const APPLICATION_VND_DVB_PFR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_PFR);
    pub const APPLICATION_VND_DVB_SERVICE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DVB_SERVICE);
    pub const APPLICATION_VND_DXR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DXR);
    pub const APPLICATION_VND_DYNAGEO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DYNAGEO);
    pub const APPLICATION_VND_DZR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_DZR);
    pub const APPLICATION_VND_EASYKARAOKE_CDGDOWNLOAD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EASYKARAOKE_CDGDOWNLOAD);
    pub const APPLICATION_VND_ECDIS_UPDATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ECDIS_UPDATE);
    pub const APPLICATION_VND_ECIP_RLP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ECIP_RLP);
    pub const APPLICATION_VND_ECLIPSE_DITTO_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ECLIPSE_DITTO_JSON);
    pub const APPLICATION_VND_ECOWIN_CHART: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ECOWIN_CHART);
    pub const APPLICATION_VND_ECOWIN_FILEREQUEST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ECOWIN_FILEREQUEST);
    pub const APPLICATION_VND_ECOWIN_FILEUPDATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ECOWIN_FILEUPDATE);
    pub const APPLICATION_VND_ECOWIN_SERIES: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ECOWIN_SERIES);
    pub const APPLICATION_VND_ECOWIN_SERIESREQUEST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ECOWIN_SERIESREQUEST);
    pub const APPLICATION_VND_ECOWIN_SERIESUPDATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ECOWIN_SERIESUPDATE);
    pub const APPLICATION_VND_EFI_IMG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EFI_IMG);
    pub const APPLICATION_VND_EFI_ISO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EFI_ISO);
    pub const APPLICATION_VND_ELN_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ELN_ZIP);
    pub const APPLICATION_VND_EMCLIENT_ACCESSREQUEST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EMCLIENT_ACCESSREQUEST_XML);
    pub const APPLICATION_VND_ENLIVEN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ENLIVEN);
    pub const APPLICATION_VND_ENPHASE_ENVOY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ENPHASE_ENVOY);
    pub const APPLICATION_VND_EPRINTS_DATA_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EPRINTS_DATA_XML);
    pub const APPLICATION_VND_EPSON_ESF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EPSON_ESF);
    pub const APPLICATION_VND_EPSON_MSF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EPSON_MSF);
    pub const APPLICATION_VND_EPSON_QUICKANIME: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EPSON_QUICKANIME);
    pub const APPLICATION_VND_EPSON_SALT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EPSON_SALT);
    pub const APPLICATION_VND_EPSON_SSF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EPSON_SSF);
    pub const APPLICATION_VND_ERICSSON_QUICKCALL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ERICSSON_QUICKCALL);
    pub const APPLICATION_VND_EROFS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EROFS);
    pub const APPLICATION_VND_ESPASS_ESPASS_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ESPASS_ESPASS_ZIP);
    pub const APPLICATION_VND_ESZIGNO3_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ESZIGNO3_XML);
    pub const APPLICATION_VND_ETSI_AOC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_AOC_XML);
    pub const APPLICATION_VND_ETSI_ASIC_E_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_ASIC_E_ZIP);
    pub const APPLICATION_VND_ETSI_ASIC_S_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_ASIC_S_ZIP);
    pub const APPLICATION_VND_ETSI_CUG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_CUG_XML);
    pub const APPLICATION_VND_ETSI_IPTVCOMMAND_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_IPTVCOMMAND_XML);
    pub const APPLICATION_VND_ETSI_IPTVDISCOVERY_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_IPTVDISCOVERY_XML);
    pub const APPLICATION_VND_ETSI_IPTVPROFILE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_IPTVPROFILE_XML);
    pub const APPLICATION_VND_ETSI_IPTVSAD_BC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_IPTVSAD_BC_XML);
    pub const APPLICATION_VND_ETSI_IPTVSAD_COD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_IPTVSAD_COD_XML);
    pub const APPLICATION_VND_ETSI_IPTVSAD_NPVR_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_IPTVSAD_NPVR_XML);
    pub const APPLICATION_VND_ETSI_IPTVSERVICE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_IPTVSERVICE_XML);
    pub const APPLICATION_VND_ETSI_IPTVSYNC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_IPTVSYNC_XML);
    pub const APPLICATION_VND_ETSI_IPTVUEPROFILE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_IPTVUEPROFILE_XML);
    pub const APPLICATION_VND_ETSI_MCID_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_MCID_XML);
    pub const APPLICATION_VND_ETSI_MHEG5: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_MHEG5);
    pub const APPLICATION_VND_ETSI_OVERLOAD_CONTROL_POLICY_DATASET_XML: Encoding = Encoding::new(
        IanaEncodingMapping::APPLICATION_VND_ETSI_OVERLOAD_CONTROL_POLICY_DATASET_XML,
    );
    pub const APPLICATION_VND_ETSI_PSTN_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_PSTN_XML);
    pub const APPLICATION_VND_ETSI_SCI_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_SCI_XML);
    pub const APPLICATION_VND_ETSI_SIMSERVS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_SIMSERVS_XML);
    pub const APPLICATION_VND_ETSI_TIMESTAMP_TOKEN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_TIMESTAMP_TOKEN);
    pub const APPLICATION_VND_ETSI_TSL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_TSL_XML);
    pub const APPLICATION_VND_ETSI_TSL_DER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ETSI_TSL_DER);
    pub const APPLICATION_VND_EU_KASPARIAN_CAR_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EU_KASPARIAN_CAR_JSON);
    pub const APPLICATION_VND_EUDORA_DATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EUDORA_DATA);
    pub const APPLICATION_VND_EVOLV_ECIG_PROFILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EVOLV_ECIG_PROFILE);
    pub const APPLICATION_VND_EVOLV_ECIG_SETTINGS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EVOLV_ECIG_SETTINGS);
    pub const APPLICATION_VND_EVOLV_ECIG_THEME: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EVOLV_ECIG_THEME);
    pub const APPLICATION_VND_EXSTREAM_EMPOWER_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EXSTREAM_EMPOWER_ZIP);
    pub const APPLICATION_VND_EXSTREAM_PACKAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EXSTREAM_PACKAGE);
    pub const APPLICATION_VND_EZPIX_ALBUM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EZPIX_ALBUM);
    pub const APPLICATION_VND_EZPIX_PACKAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_EZPIX_PACKAGE);
    pub const APPLICATION_VND_F_SECURE_MOBILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_F_SECURE_MOBILE);
    pub const APPLICATION_VND_FAMILYSEARCH_GEDCOM_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FAMILYSEARCH_GEDCOM_ZIP);
    pub const APPLICATION_VND_FASTCOPY_DISK_IMAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FASTCOPY_DISK_IMAGE);
    pub const APPLICATION_VND_FDSN_MSEED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FDSN_MSEED);
    pub const APPLICATION_VND_FDSN_SEED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FDSN_SEED);
    pub const APPLICATION_VND_FFSNS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FFSNS);
    pub const APPLICATION_VND_FICLAB_FLB_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FICLAB_FLB_ZIP);
    pub const APPLICATION_VND_FILMIT_ZFC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FILMIT_ZFC);
    pub const APPLICATION_VND_FINTS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FINTS);
    pub const APPLICATION_VND_FIREMONKEYS_CLOUDCELL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FIREMONKEYS_CLOUDCELL);
    pub const APPLICATION_VND_FLUXTIME_CLIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FLUXTIME_CLIP);
    pub const APPLICATION_VND_FONT_FONTFORGE_SFD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FONT_FONTFORGE_SFD);
    pub const APPLICATION_VND_FRAMEMAKER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FRAMEMAKER);
    pub const APPLICATION_VND_FREELOG_COMIC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FREELOG_COMIC);
    pub const APPLICATION_VND_FROGANS_FNC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FROGANS_FNC);
    pub const APPLICATION_VND_FROGANS_LTF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FROGANS_LTF);
    pub const APPLICATION_VND_FSC_WEBLAUNCH: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FSC_WEBLAUNCH);
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJIFILM_FB_DOCUWORKS);
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_BINDER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_BINDER);
    pub const APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_CONTAINER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJIFILM_FB_DOCUWORKS_CONTAINER);
    pub const APPLICATION_VND_FUJIFILM_FB_JFI_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJIFILM_FB_JFI_XML);
    pub const APPLICATION_VND_FUJITSU_OASYS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJITSU_OASYS);
    pub const APPLICATION_VND_FUJITSU_OASYS2: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJITSU_OASYS2);
    pub const APPLICATION_VND_FUJITSU_OASYS3: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJITSU_OASYS3);
    pub const APPLICATION_VND_FUJITSU_OASYSGP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJITSU_OASYSGP);
    pub const APPLICATION_VND_FUJITSU_OASYSPRS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJITSU_OASYSPRS);
    pub const APPLICATION_VND_FUJIXEROX_ART_EX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJIXEROX_ART_EX);
    pub const APPLICATION_VND_FUJIXEROX_ART4: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJIXEROX_ART4);
    pub const APPLICATION_VND_FUJIXEROX_HBPL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJIXEROX_HBPL);
    pub const APPLICATION_VND_FUJIXEROX_DDD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJIXEROX_DDD);
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJIXEROX_DOCUWORKS);
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS_BINDER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJIXEROX_DOCUWORKS_BINDER);
    pub const APPLICATION_VND_FUJIXEROX_DOCUWORKS_CONTAINER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUJIXEROX_DOCUWORKS_CONTAINER);
    pub const APPLICATION_VND_FUT_MISNET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUT_MISNET);
    pub const APPLICATION_VND_FUTOIN_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUTOIN_CBOR);
    pub const APPLICATION_VND_FUTOIN_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUTOIN_JSON);
    pub const APPLICATION_VND_FUZZYSHEET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_FUZZYSHEET);
    pub const APPLICATION_VND_GENOMATIX_TUXEDO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GENOMATIX_TUXEDO);
    pub const APPLICATION_VND_GENOZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GENOZIP);
    pub const APPLICATION_VND_GENTICS_GRD_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GENTICS_GRD_JSON);
    pub const APPLICATION_VND_GENTOO_CATMETADATA_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GENTOO_CATMETADATA_XML);
    pub const APPLICATION_VND_GENTOO_EBUILD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GENTOO_EBUILD);
    pub const APPLICATION_VND_GENTOO_ECLASS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GENTOO_ECLASS);
    pub const APPLICATION_VND_GENTOO_GPKG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GENTOO_GPKG);
    pub const APPLICATION_VND_GENTOO_MANIFEST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GENTOO_MANIFEST);
    pub const APPLICATION_VND_GENTOO_PKGMETADATA_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GENTOO_PKGMETADATA_XML);
    pub const APPLICATION_VND_GENTOO_XPAK: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GENTOO_XPAK);
    pub const APPLICATION_VND_GEO_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GEO_JSON);
    pub const APPLICATION_VND_GEOCUBE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GEOCUBE_XML);
    pub const APPLICATION_VND_GEOGEBRA_FILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GEOGEBRA_FILE);
    pub const APPLICATION_VND_GEOGEBRA_SLIDES: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GEOGEBRA_SLIDES);
    pub const APPLICATION_VND_GEOGEBRA_TOOL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GEOGEBRA_TOOL);
    pub const APPLICATION_VND_GEOMETRY_EXPLORER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GEOMETRY_EXPLORER);
    pub const APPLICATION_VND_GEONEXT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GEONEXT);
    pub const APPLICATION_VND_GEOPLAN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GEOPLAN);
    pub const APPLICATION_VND_GEOSPACE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GEOSPACE);
    pub const APPLICATION_VND_GERBER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GERBER);
    pub const APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT);
    pub const APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT_RESPONSE: Encoding = Encoding::new(
        IanaEncodingMapping::APPLICATION_VND_GLOBALPLATFORM_CARD_CONTENT_MGT_RESPONSE,
    );
    pub const APPLICATION_VND_GMX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GMX);
    pub const APPLICATION_VND_GNU_TALER_EXCHANGE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GNU_TALER_EXCHANGE_JSON);
    pub const APPLICATION_VND_GNU_TALER_MERCHANT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GNU_TALER_MERCHANT_JSON);
    pub const APPLICATION_VND_GOOGLE_EARTH_KML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GOOGLE_EARTH_KML_XML);
    pub const APPLICATION_VND_GOOGLE_EARTH_KMZ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GOOGLE_EARTH_KMZ);
    pub const APPLICATION_VND_GOV_SK_E_FORM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GOV_SK_E_FORM_XML);
    pub const APPLICATION_VND_GOV_SK_E_FORM_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GOV_SK_E_FORM_ZIP);
    pub const APPLICATION_VND_GOV_SK_XMLDATACONTAINER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GOV_SK_XMLDATACONTAINER_XML);
    pub const APPLICATION_VND_GPXSEE_MAP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GPXSEE_MAP_XML);
    pub const APPLICATION_VND_GRAFEQ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GRAFEQ);
    pub const APPLICATION_VND_GRIDMP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GRIDMP);
    pub const APPLICATION_VND_GROOVE_ACCOUNT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GROOVE_ACCOUNT);
    pub const APPLICATION_VND_GROOVE_HELP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GROOVE_HELP);
    pub const APPLICATION_VND_GROOVE_IDENTITY_MESSAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GROOVE_IDENTITY_MESSAGE);
    pub const APPLICATION_VND_GROOVE_INJECTOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GROOVE_INJECTOR);
    pub const APPLICATION_VND_GROOVE_TOOL_MESSAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GROOVE_TOOL_MESSAGE);
    pub const APPLICATION_VND_GROOVE_TOOL_TEMPLATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GROOVE_TOOL_TEMPLATE);
    pub const APPLICATION_VND_GROOVE_VCARD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_GROOVE_VCARD);
    pub const APPLICATION_VND_HAL_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HAL_JSON);
    pub const APPLICATION_VND_HAL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HAL_XML);
    pub const APPLICATION_VND_HBCI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HBCI);
    pub const APPLICATION_VND_HC_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HC_JSON);
    pub const APPLICATION_VND_HCL_BIREPORTS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HCL_BIREPORTS);
    pub const APPLICATION_VND_HDT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HDT);
    pub const APPLICATION_VND_HEROKU_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HEROKU_JSON);
    pub const APPLICATION_VND_HHE_LESSON_PLAYER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HHE_LESSON_PLAYER);
    pub const APPLICATION_VND_HP_HPGL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HP_HPGL);
    pub const APPLICATION_VND_HP_PCL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HP_PCL);
    pub const APPLICATION_VND_HP_PCLXL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HP_PCLXL);
    pub const APPLICATION_VND_HP_HPID: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HP_HPID);
    pub const APPLICATION_VND_HP_HPS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HP_HPS);
    pub const APPLICATION_VND_HP_JLYT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HP_JLYT);
    pub const APPLICATION_VND_HSL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HSL);
    pub const APPLICATION_VND_HTTPHONE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HTTPHONE);
    pub const APPLICATION_VND_HYDROSTATIX_SOF_DATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HYDROSTATIX_SOF_DATA);
    pub const APPLICATION_VND_HYPER_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HYPER_JSON);
    pub const APPLICATION_VND_HYPER_ITEM_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HYPER_ITEM_JSON);
    pub const APPLICATION_VND_HYPERDRIVE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HYPERDRIVE_JSON);
    pub const APPLICATION_VND_HZN_3D_CROSSWORD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_HZN_3D_CROSSWORD);
    pub const APPLICATION_VND_IBM_MINIPAY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IBM_MINIPAY);
    pub const APPLICATION_VND_IBM_AFPLINEDATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IBM_AFPLINEDATA);
    pub const APPLICATION_VND_IBM_ELECTRONIC_MEDIA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IBM_ELECTRONIC_MEDIA);
    pub const APPLICATION_VND_IBM_MODCAP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IBM_MODCAP);
    pub const APPLICATION_VND_IBM_RIGHTS_MANAGEMENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IBM_RIGHTS_MANAGEMENT);
    pub const APPLICATION_VND_IBM_SECURE_CONTAINER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IBM_SECURE_CONTAINER);
    pub const APPLICATION_VND_ICCPROFILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ICCPROFILE);
    pub const APPLICATION_VND_IEEE_1905: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IEEE_1905);
    pub const APPLICATION_VND_IGLOADER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IGLOADER);
    pub const APPLICATION_VND_IMAGEMETER_FOLDER_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMAGEMETER_FOLDER_ZIP);
    pub const APPLICATION_VND_IMAGEMETER_IMAGE_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMAGEMETER_IMAGE_ZIP);
    pub const APPLICATION_VND_IMMERVISION_IVP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMMERVISION_IVP);
    pub const APPLICATION_VND_IMMERVISION_IVU: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMMERVISION_IVU);
    pub const APPLICATION_VND_IMS_IMSCCV1P1: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMS_IMSCCV1P1);
    pub const APPLICATION_VND_IMS_IMSCCV1P2: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMS_IMSCCV1P2);
    pub const APPLICATION_VND_IMS_IMSCCV1P3: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMS_IMSCCV1P3);
    pub const APPLICATION_VND_IMS_LIS_V2_RESULT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMS_LIS_V2_RESULT_JSON);
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLCONSUMERPROFILE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMS_LTI_V2_TOOLCONSUMERPROFILE_JSON);
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_JSON);
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_ID_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMS_LTI_V2_TOOLPROXY_ID_JSON);
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_JSON);
    pub const APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_SIMPLE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IMS_LTI_V2_TOOLSETTINGS_SIMPLE_JSON);
    pub const APPLICATION_VND_INFORMEDCONTROL_RMS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INFORMEDCONTROL_RMS_XML);
    pub const APPLICATION_VND_INFORMIX_VISIONARY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INFORMIX_VISIONARY);
    pub const APPLICATION_VND_INFOTECH_PROJECT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INFOTECH_PROJECT);
    pub const APPLICATION_VND_INFOTECH_PROJECT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INFOTECH_PROJECT_XML);
    pub const APPLICATION_VND_INNOPATH_WAMP_NOTIFICATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INNOPATH_WAMP_NOTIFICATION);
    pub const APPLICATION_VND_INSORS_IGM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INSORS_IGM);
    pub const APPLICATION_VND_INTERCON_FORMNET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INTERCON_FORMNET);
    pub const APPLICATION_VND_INTERGEO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INTERGEO);
    pub const APPLICATION_VND_INTERTRUST_DIGIBOX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INTERTRUST_DIGIBOX);
    pub const APPLICATION_VND_INTERTRUST_NNCP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INTERTRUST_NNCP);
    pub const APPLICATION_VND_INTU_QBO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INTU_QBO);
    pub const APPLICATION_VND_INTU_QFX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_INTU_QFX);
    pub const APPLICATION_VND_IPFS_IPNS_RECORD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPFS_IPNS_RECORD);
    pub const APPLICATION_VND_IPLD_CAR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPLD_CAR);
    pub const APPLICATION_VND_IPLD_DAG_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPLD_DAG_CBOR);
    pub const APPLICATION_VND_IPLD_DAG_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPLD_DAG_JSON);
    pub const APPLICATION_VND_IPLD_RAW: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPLD_RAW);
    pub const APPLICATION_VND_IPTC_G2_CATALOGITEM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPTC_G2_CATALOGITEM_XML);
    pub const APPLICATION_VND_IPTC_G2_CONCEPTITEM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPTC_G2_CONCEPTITEM_XML);
    pub const APPLICATION_VND_IPTC_G2_KNOWLEDGEITEM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPTC_G2_KNOWLEDGEITEM_XML);
    pub const APPLICATION_VND_IPTC_G2_NEWSITEM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPTC_G2_NEWSITEM_XML);
    pub const APPLICATION_VND_IPTC_G2_NEWSMESSAGE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPTC_G2_NEWSMESSAGE_XML);
    pub const APPLICATION_VND_IPTC_G2_PACKAGEITEM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPTC_G2_PACKAGEITEM_XML);
    pub const APPLICATION_VND_IPTC_G2_PLANNINGITEM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPTC_G2_PLANNINGITEM_XML);
    pub const APPLICATION_VND_IPUNPLUGGED_RCPROFILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IPUNPLUGGED_RCPROFILE);
    pub const APPLICATION_VND_IREPOSITORY_PACKAGE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IREPOSITORY_PACKAGE_XML);
    pub const APPLICATION_VND_IS_XPR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_IS_XPR);
    pub const APPLICATION_VND_ISAC_FCS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ISAC_FCS);
    pub const APPLICATION_VND_ISO11783_10_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ISO11783_10_ZIP);
    pub const APPLICATION_VND_JAM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JAM);
    pub const APPLICATION_VND_JAPANNET_DIRECTORY_SERVICE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JAPANNET_DIRECTORY_SERVICE);
    pub const APPLICATION_VND_JAPANNET_JPNSTORE_WAKEUP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JAPANNET_JPNSTORE_WAKEUP);
    pub const APPLICATION_VND_JAPANNET_PAYMENT_WAKEUP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JAPANNET_PAYMENT_WAKEUP);
    pub const APPLICATION_VND_JAPANNET_REGISTRATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JAPANNET_REGISTRATION);
    pub const APPLICATION_VND_JAPANNET_REGISTRATION_WAKEUP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JAPANNET_REGISTRATION_WAKEUP);
    pub const APPLICATION_VND_JAPANNET_SETSTORE_WAKEUP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JAPANNET_SETSTORE_WAKEUP);
    pub const APPLICATION_VND_JAPANNET_VERIFICATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JAPANNET_VERIFICATION);
    pub const APPLICATION_VND_JAPANNET_VERIFICATION_WAKEUP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JAPANNET_VERIFICATION_WAKEUP);
    pub const APPLICATION_VND_JCP_JAVAME_MIDLET_RMS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JCP_JAVAME_MIDLET_RMS);
    pub const APPLICATION_VND_JISP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JISP);
    pub const APPLICATION_VND_JOOST_JODA_ARCHIVE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JOOST_JODA_ARCHIVE);
    pub const APPLICATION_VND_JSK_ISDN_NGN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_JSK_ISDN_NGN);
    pub const APPLICATION_VND_KAHOOTZ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KAHOOTZ);
    pub const APPLICATION_VND_KDE_KARBON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KDE_KARBON);
    pub const APPLICATION_VND_KDE_KCHART: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KDE_KCHART);
    pub const APPLICATION_VND_KDE_KFORMULA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KDE_KFORMULA);
    pub const APPLICATION_VND_KDE_KIVIO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KDE_KIVIO);
    pub const APPLICATION_VND_KDE_KONTOUR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KDE_KONTOUR);
    pub const APPLICATION_VND_KDE_KPRESENTER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KDE_KPRESENTER);
    pub const APPLICATION_VND_KDE_KSPREAD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KDE_KSPREAD);
    pub const APPLICATION_VND_KDE_KWORD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KDE_KWORD);
    pub const APPLICATION_VND_KENAMEAAPP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KENAMEAAPP);
    pub const APPLICATION_VND_KIDSPIRATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KIDSPIRATION);
    pub const APPLICATION_VND_KOAN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KOAN);
    pub const APPLICATION_VND_KODAK_DESCRIPTOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_KODAK_DESCRIPTOR);
    pub const APPLICATION_VND_LAS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LAS);
    pub const APPLICATION_VND_LAS_LAS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LAS_LAS_JSON);
    pub const APPLICATION_VND_LAS_LAS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LAS_LAS_XML);
    pub const APPLICATION_VND_LASZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LASZIP);
    pub const APPLICATION_VND_LDEV_PRODUCTLICENSING: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LDEV_PRODUCTLICENSING);
    pub const APPLICATION_VND_LEAP_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LEAP_JSON);
    pub const APPLICATION_VND_LIBERTY_REQUEST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LIBERTY_REQUEST_XML);
    pub const APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_DESKTOP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_DESKTOP);
    pub const APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_EXCHANGE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LLAMAGRAPHICS_LIFE_BALANCE_EXCHANGE_XML);
    pub const APPLICATION_VND_LOGIPIPE_CIRCUIT_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LOGIPIPE_CIRCUIT_ZIP);
    pub const APPLICATION_VND_LOOM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LOOM);
    pub const APPLICATION_VND_LOTUS_1_2_3: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LOTUS_1_2_3);
    pub const APPLICATION_VND_LOTUS_APPROACH: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LOTUS_APPROACH);
    pub const APPLICATION_VND_LOTUS_FREELANCE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LOTUS_FREELANCE);
    pub const APPLICATION_VND_LOTUS_NOTES: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LOTUS_NOTES);
    pub const APPLICATION_VND_LOTUS_ORGANIZER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LOTUS_ORGANIZER);
    pub const APPLICATION_VND_LOTUS_SCREENCAM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LOTUS_SCREENCAM);
    pub const APPLICATION_VND_LOTUS_WORDPRO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_LOTUS_WORDPRO);
    pub const APPLICATION_VND_MACPORTS_PORTPKG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MACPORTS_PORTPKG);
    pub const APPLICATION_VND_MAPBOX_VECTOR_TILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MAPBOX_VECTOR_TILE);
    pub const APPLICATION_VND_MARLIN_DRM_ACTIONTOKEN_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MARLIN_DRM_ACTIONTOKEN_XML);
    pub const APPLICATION_VND_MARLIN_DRM_CONFTOKEN_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MARLIN_DRM_CONFTOKEN_XML);
    pub const APPLICATION_VND_MARLIN_DRM_LICENSE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MARLIN_DRM_LICENSE_XML);
    pub const APPLICATION_VND_MARLIN_DRM_MDCF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MARLIN_DRM_MDCF);
    pub const APPLICATION_VND_MASON_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MASON_JSON);
    pub const APPLICATION_VND_MAXAR_ARCHIVE_3TZ_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MAXAR_ARCHIVE_3TZ_ZIP);
    pub const APPLICATION_VND_MAXMIND_MAXMIND_DB: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MAXMIND_MAXMIND_DB);
    pub const APPLICATION_VND_MCD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MCD);
    pub const APPLICATION_VND_MDL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MDL);
    pub const APPLICATION_VND_MDL_MBSDF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MDL_MBSDF);
    pub const APPLICATION_VND_MEDCALCDATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MEDCALCDATA);
    pub const APPLICATION_VND_MEDIASTATION_CDKEY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MEDIASTATION_CDKEY);
    pub const APPLICATION_VND_MEDICALHOLODECK_RECORDXR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MEDICALHOLODECK_RECORDXR);
    pub const APPLICATION_VND_MERIDIAN_SLINGSHOT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MERIDIAN_SLINGSHOT);
    pub const APPLICATION_VND_MERMAID: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MERMAID);
    pub const APPLICATION_VND_MFMP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MFMP);
    pub const APPLICATION_VND_MICRO_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MICRO_JSON);
    pub const APPLICATION_VND_MICROGRAFX_FLO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MICROGRAFX_FLO);
    pub const APPLICATION_VND_MICROGRAFX_IGX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MICROGRAFX_IGX);
    pub const APPLICATION_VND_MICROSOFT_PORTABLE_EXECUTABLE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MICROSOFT_PORTABLE_EXECUTABLE);
    pub const APPLICATION_VND_MICROSOFT_WINDOWS_THUMBNAIL_CACHE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MICROSOFT_WINDOWS_THUMBNAIL_CACHE);
    pub const APPLICATION_VND_MIELE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MIELE_JSON);
    pub const APPLICATION_VND_MIF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MIF);
    pub const APPLICATION_VND_MINISOFT_HP3000_SAVE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MINISOFT_HP3000_SAVE);
    pub const APPLICATION_VND_MITSUBISHI_MISTY_GUARD_TRUSTWEB: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MITSUBISHI_MISTY_GUARD_TRUSTWEB);
    pub const APPLICATION_VND_MODL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MODL);
    pub const APPLICATION_VND_MOPHUN_APPLICATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOPHUN_APPLICATION);
    pub const APPLICATION_VND_MOPHUN_CERTIFICATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOPHUN_CERTIFICATE);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOTOROLA_FLEXSUITE);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_ADSI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOTOROLA_FLEXSUITE_ADSI);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_FIS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOTOROLA_FLEXSUITE_FIS);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_GOTAP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOTOROLA_FLEXSUITE_GOTAP);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_KMR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOTOROLA_FLEXSUITE_KMR);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_TTC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOTOROLA_FLEXSUITE_TTC);
    pub const APPLICATION_VND_MOTOROLA_FLEXSUITE_WEM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOTOROLA_FLEXSUITE_WEM);
    pub const APPLICATION_VND_MOTOROLA_IPRM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOTOROLA_IPRM);
    pub const APPLICATION_VND_MOZILLA_XUL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MOZILLA_XUL_XML);
    pub const APPLICATION_VND_MS_3MFDOCUMENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_3MFDOCUMENT);
    pub const APPLICATION_VND_MS_PRINTDEVICECAPABILITIES_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_PRINTDEVICECAPABILITIES_XML);
    pub const APPLICATION_VND_MS_PRINTSCHEMATICKET_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_PRINTSCHEMATICKET_XML);
    pub const APPLICATION_VND_MS_ARTGALRY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_ARTGALRY);
    pub const APPLICATION_VND_MS_ASF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_ASF);
    pub const APPLICATION_VND_MS_CAB_COMPRESSED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_CAB_COMPRESSED);
    pub const APPLICATION_VND_MS_EXCEL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_EXCEL);
    pub const APPLICATION_VND_MS_EXCEL_ADDIN_MACROENABLED_12: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_EXCEL_ADDIN_MACROENABLED_12);
    pub const APPLICATION_VND_MS_EXCEL_SHEET_BINARY_MACROENABLED_12: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_EXCEL_SHEET_BINARY_MACROENABLED_12);
    pub const APPLICATION_VND_MS_EXCEL_SHEET_MACROENABLED_12: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_EXCEL_SHEET_MACROENABLED_12);
    pub const APPLICATION_VND_MS_EXCEL_TEMPLATE_MACROENABLED_12: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_EXCEL_TEMPLATE_MACROENABLED_12);
    pub const APPLICATION_VND_MS_FONTOBJECT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_FONTOBJECT);
    pub const APPLICATION_VND_MS_HTMLHELP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_HTMLHELP);
    pub const APPLICATION_VND_MS_IMS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_IMS);
    pub const APPLICATION_VND_MS_LRM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_LRM);
    pub const APPLICATION_VND_MS_OFFICE_ACTIVEX_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_OFFICE_ACTIVEX_XML);
    pub const APPLICATION_VND_MS_OFFICETHEME: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_OFFICETHEME);
    pub const APPLICATION_VND_MS_PLAYREADY_INITIATOR_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_PLAYREADY_INITIATOR_XML);
    pub const APPLICATION_VND_MS_POWERPOINT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_POWERPOINT);
    pub const APPLICATION_VND_MS_POWERPOINT_ADDIN_MACROENABLED_12: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_POWERPOINT_ADDIN_MACROENABLED_12);
    pub const APPLICATION_VND_MS_POWERPOINT_PRESENTATION_MACROENABLED_12: Encoding = Encoding::new(
        IanaEncodingMapping::APPLICATION_VND_MS_POWERPOINT_PRESENTATION_MACROENABLED_12,
    );
    pub const APPLICATION_VND_MS_POWERPOINT_SLIDE_MACROENABLED_12: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_POWERPOINT_SLIDE_MACROENABLED_12);
    pub const APPLICATION_VND_MS_POWERPOINT_SLIDESHOW_MACROENABLED_12: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_POWERPOINT_SLIDESHOW_MACROENABLED_12);
    pub const APPLICATION_VND_MS_POWERPOINT_TEMPLATE_MACROENABLED_12: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_POWERPOINT_TEMPLATE_MACROENABLED_12);
    pub const APPLICATION_VND_MS_PROJECT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_PROJECT);
    pub const APPLICATION_VND_MS_TNEF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_TNEF);
    pub const APPLICATION_VND_MS_WINDOWS_DEVICEPAIRING: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WINDOWS_DEVICEPAIRING);
    pub const APPLICATION_VND_MS_WINDOWS_NWPRINTING_OOB: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WINDOWS_NWPRINTING_OOB);
    pub const APPLICATION_VND_MS_WINDOWS_PRINTERPAIRING: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WINDOWS_PRINTERPAIRING);
    pub const APPLICATION_VND_MS_WINDOWS_WSD_OOB: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WINDOWS_WSD_OOB);
    pub const APPLICATION_VND_MS_WMDRM_LIC_CHLG_REQ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WMDRM_LIC_CHLG_REQ);
    pub const APPLICATION_VND_MS_WMDRM_LIC_RESP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WMDRM_LIC_RESP);
    pub const APPLICATION_VND_MS_WMDRM_METER_CHLG_REQ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WMDRM_METER_CHLG_REQ);
    pub const APPLICATION_VND_MS_WMDRM_METER_RESP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WMDRM_METER_RESP);
    pub const APPLICATION_VND_MS_WORD_DOCUMENT_MACROENABLED_12: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WORD_DOCUMENT_MACROENABLED_12);
    pub const APPLICATION_VND_MS_WORD_TEMPLATE_MACROENABLED_12: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WORD_TEMPLATE_MACROENABLED_12);
    pub const APPLICATION_VND_MS_WORKS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WORKS);
    pub const APPLICATION_VND_MS_WPL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_WPL);
    pub const APPLICATION_VND_MS_XPSDOCUMENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MS_XPSDOCUMENT);
    pub const APPLICATION_VND_MSA_DISK_IMAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MSA_DISK_IMAGE);
    pub const APPLICATION_VND_MSEQ: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MSEQ);
    pub const APPLICATION_VND_MSIGN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MSIGN);
    pub const APPLICATION_VND_MULTIAD_CREATOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MULTIAD_CREATOR);
    pub const APPLICATION_VND_MULTIAD_CREATOR_CIF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MULTIAD_CREATOR_CIF);
    pub const APPLICATION_VND_MUSIC_NIFF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MUSIC_NIFF);
    pub const APPLICATION_VND_MUSICIAN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MUSICIAN);
    pub const APPLICATION_VND_MUVEE_STYLE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MUVEE_STYLE);
    pub const APPLICATION_VND_MYNFC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_MYNFC);
    pub const APPLICATION_VND_NACAMAR_YBRID_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NACAMAR_YBRID_JSON);
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NATO_BINDINGDATAOBJECT_CBOR);
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NATO_BINDINGDATAOBJECT_JSON);
    pub const APPLICATION_VND_NATO_BINDINGDATAOBJECT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NATO_BINDINGDATAOBJECT_XML);
    pub const APPLICATION_VND_NATO_OPENXMLFORMATS_PACKAGE_IEPD_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NATO_OPENXMLFORMATS_PACKAGE_IEPD_ZIP);
    pub const APPLICATION_VND_NCD_CONTROL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NCD_CONTROL);
    pub const APPLICATION_VND_NCD_REFERENCE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NCD_REFERENCE);
    pub const APPLICATION_VND_NEARST_INV_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NEARST_INV_JSON);
    pub const APPLICATION_VND_NEBUMIND_LINE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NEBUMIND_LINE);
    pub const APPLICATION_VND_NERVANA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NERVANA);
    pub const APPLICATION_VND_NETFPX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NETFPX);
    pub const APPLICATION_VND_NEUROLANGUAGE_NLU: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NEUROLANGUAGE_NLU);
    pub const APPLICATION_VND_NIMN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NIMN);
    pub const APPLICATION_VND_NINTENDO_NITRO_ROM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NINTENDO_NITRO_ROM);
    pub const APPLICATION_VND_NINTENDO_SNES_ROM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NINTENDO_SNES_ROM);
    pub const APPLICATION_VND_NITF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NITF);
    pub const APPLICATION_VND_NOBLENET_DIRECTORY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOBLENET_DIRECTORY);
    pub const APPLICATION_VND_NOBLENET_SEALER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOBLENET_SEALER);
    pub const APPLICATION_VND_NOBLENET_WEB: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOBLENET_WEB);
    pub const APPLICATION_VND_NOKIA_CATALOGS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_CATALOGS);
    pub const APPLICATION_VND_NOKIA_CONML_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_CONML_WBXML);
    pub const APPLICATION_VND_NOKIA_CONML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_CONML_XML);
    pub const APPLICATION_VND_NOKIA_ISDS_RADIO_PRESETS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_ISDS_RADIO_PRESETS);
    pub const APPLICATION_VND_NOKIA_IPTV_CONFIG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_IPTV_CONFIG_XML);
    pub const APPLICATION_VND_NOKIA_LANDMARK_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_LANDMARK_WBXML);
    pub const APPLICATION_VND_NOKIA_LANDMARK_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_LANDMARK_XML);
    pub const APPLICATION_VND_NOKIA_LANDMARKCOLLECTION_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_LANDMARKCOLLECTION_XML);
    pub const APPLICATION_VND_NOKIA_N_GAGE_AC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_N_GAGE_AC_XML);
    pub const APPLICATION_VND_NOKIA_N_GAGE_DATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_N_GAGE_DATA);
    pub const APPLICATION_VND_NOKIA_N_GAGE_SYMBIAN_INSTALL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_N_GAGE_SYMBIAN_INSTALL);
    pub const APPLICATION_VND_NOKIA_NCD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_NCD);
    pub const APPLICATION_VND_NOKIA_PCD_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_PCD_WBXML);
    pub const APPLICATION_VND_NOKIA_PCD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_PCD_XML);
    pub const APPLICATION_VND_NOKIA_RADIO_PRESET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_RADIO_PRESET);
    pub const APPLICATION_VND_NOKIA_RADIO_PRESETS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOKIA_RADIO_PRESETS);
    pub const APPLICATION_VND_NOVADIGM_EDM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOVADIGM_EDM);
    pub const APPLICATION_VND_NOVADIGM_EDX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOVADIGM_EDX);
    pub const APPLICATION_VND_NOVADIGM_EXT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NOVADIGM_EXT);
    pub const APPLICATION_VND_NTT_LOCAL_CONTENT_SHARE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NTT_LOCAL_CONTENT_SHARE);
    pub const APPLICATION_VND_NTT_LOCAL_FILE_TRANSFER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NTT_LOCAL_FILE_TRANSFER);
    pub const APPLICATION_VND_NTT_LOCAL_OGW_REMOTE_ACCESS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NTT_LOCAL_OGW_REMOTE_ACCESS);
    pub const APPLICATION_VND_NTT_LOCAL_SIP_TA_REMOTE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NTT_LOCAL_SIP_TA_REMOTE);
    pub const APPLICATION_VND_NTT_LOCAL_SIP_TA_TCP_STREAM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_NTT_LOCAL_SIP_TA_TCP_STREAM);
    pub const APPLICATION_VND_OAI_WORKFLOWS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OAI_WORKFLOWS);
    pub const APPLICATION_VND_OAI_WORKFLOWS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OAI_WORKFLOWS_JSON);
    pub const APPLICATION_VND_OAI_WORKFLOWS_YAML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OAI_WORKFLOWS_YAML);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_BASE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_BASE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_CHART: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_CHART);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_CHART_TEMPLATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_CHART_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_DATABASE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_DATABASE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA_TEMPLATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_FORMULA_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS_TEMPLATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_GRAPHICS_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE_TEMPLATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_IMAGE_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION_TEMPLATE: Encoding = Encoding::new(
        IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_PRESENTATION_TEMPLATE,
    );
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET_TEMPLATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_SPREADSHEET_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER_TEMPLATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_MASTER_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_TEMPLATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_TEMPLATE);
    pub const APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_WEB: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OASIS_OPENDOCUMENT_TEXT_WEB);
    pub const APPLICATION_VND_OBN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OBN);
    pub const APPLICATION_VND_OCF_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OCF_CBOR);
    pub const APPLICATION_VND_OCI_IMAGE_MANIFEST_V1_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OCI_IMAGE_MANIFEST_V1_JSON);
    pub const APPLICATION_VND_OFTN_L10N_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OFTN_L10N_JSON);
    pub const APPLICATION_VND_OIPF_CONTENTACCESSDOWNLOAD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OIPF_CONTENTACCESSDOWNLOAD_XML);
    pub const APPLICATION_VND_OIPF_CONTENTACCESSSTREAMING_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OIPF_CONTENTACCESSSTREAMING_XML);
    pub const APPLICATION_VND_OIPF_CSPG_HEXBINARY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OIPF_CSPG_HEXBINARY);
    pub const APPLICATION_VND_OIPF_DAE_SVG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OIPF_DAE_SVG_XML);
    pub const APPLICATION_VND_OIPF_DAE_XHTML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OIPF_DAE_XHTML_XML);
    pub const APPLICATION_VND_OIPF_MIPPVCONTROLMESSAGE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OIPF_MIPPVCONTROLMESSAGE_XML);
    pub const APPLICATION_VND_OIPF_PAE_GEM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OIPF_PAE_GEM);
    pub const APPLICATION_VND_OIPF_SPDISCOVERY_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OIPF_SPDISCOVERY_XML);
    pub const APPLICATION_VND_OIPF_SPDLIST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OIPF_SPDLIST_XML);
    pub const APPLICATION_VND_OIPF_UEPROFILE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OIPF_UEPROFILE_XML);
    pub const APPLICATION_VND_OIPF_USERPROFILE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OIPF_USERPROFILE_XML);
    pub const APPLICATION_VND_OLPC_SUGAR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OLPC_SUGAR);
    pub const APPLICATION_VND_OMA_SCWS_CONFIG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_SCWS_CONFIG);
    pub const APPLICATION_VND_OMA_SCWS_HTTP_REQUEST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_SCWS_HTTP_REQUEST);
    pub const APPLICATION_VND_OMA_SCWS_HTTP_RESPONSE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_SCWS_HTTP_RESPONSE);
    pub const APPLICATION_VND_OMA_BCAST_ASSOCIATED_PROCEDURE_PARAMETER_XML: Encoding =
        Encoding::new(
            IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_ASSOCIATED_PROCEDURE_PARAMETER_XML,
        );
    pub const APPLICATION_VND_OMA_BCAST_DRM_TRIGGER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_DRM_TRIGGER_XML);
    pub const APPLICATION_VND_OMA_BCAST_IMD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_IMD_XML);
    pub const APPLICATION_VND_OMA_BCAST_LTKM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_LTKM);
    pub const APPLICATION_VND_OMA_BCAST_NOTIFICATION_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_NOTIFICATION_XML);
    pub const APPLICATION_VND_OMA_BCAST_PROVISIONINGTRIGGER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_PROVISIONINGTRIGGER);
    pub const APPLICATION_VND_OMA_BCAST_SGBOOT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_SGBOOT);
    pub const APPLICATION_VND_OMA_BCAST_SGDD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_SGDD_XML);
    pub const APPLICATION_VND_OMA_BCAST_SGDU: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_SGDU);
    pub const APPLICATION_VND_OMA_BCAST_SIMPLE_SYMBOL_CONTAINER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_SIMPLE_SYMBOL_CONTAINER);
    pub const APPLICATION_VND_OMA_BCAST_SMARTCARD_TRIGGER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_SMARTCARD_TRIGGER_XML);
    pub const APPLICATION_VND_OMA_BCAST_SPROV_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_SPROV_XML);
    pub const APPLICATION_VND_OMA_BCAST_STKM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_BCAST_STKM);
    pub const APPLICATION_VND_OMA_CAB_ADDRESS_BOOK_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_CAB_ADDRESS_BOOK_XML);
    pub const APPLICATION_VND_OMA_CAB_FEATURE_HANDLER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_CAB_FEATURE_HANDLER_XML);
    pub const APPLICATION_VND_OMA_CAB_PCC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_CAB_PCC_XML);
    pub const APPLICATION_VND_OMA_CAB_SUBS_INVITE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_CAB_SUBS_INVITE_XML);
    pub const APPLICATION_VND_OMA_CAB_USER_PREFS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_CAB_USER_PREFS_XML);
    pub const APPLICATION_VND_OMA_DCD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_DCD);
    pub const APPLICATION_VND_OMA_DCDC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_DCDC);
    pub const APPLICATION_VND_OMA_DD2_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_DD2_XML);
    pub const APPLICATION_VND_OMA_DRM_RISD_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_DRM_RISD_XML);
    pub const APPLICATION_VND_OMA_GROUP_USAGE_LIST_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_GROUP_USAGE_LIST_XML);
    pub const APPLICATION_VND_OMA_LWM2M_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_LWM2M_CBOR);
    pub const APPLICATION_VND_OMA_LWM2M_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_LWM2M_JSON);
    pub const APPLICATION_VND_OMA_LWM2M_TLV: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_LWM2M_TLV);
    pub const APPLICATION_VND_OMA_PAL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_PAL_XML);
    pub const APPLICATION_VND_OMA_POC_DETAILED_PROGRESS_REPORT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_POC_DETAILED_PROGRESS_REPORT_XML);
    pub const APPLICATION_VND_OMA_POC_FINAL_REPORT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_POC_FINAL_REPORT_XML);
    pub const APPLICATION_VND_OMA_POC_GROUPS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_POC_GROUPS_XML);
    pub const APPLICATION_VND_OMA_POC_INVOCATION_DESCRIPTOR_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_POC_INVOCATION_DESCRIPTOR_XML);
    pub const APPLICATION_VND_OMA_POC_OPTIMIZED_PROGRESS_REPORT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_POC_OPTIMIZED_PROGRESS_REPORT_XML);
    pub const APPLICATION_VND_OMA_PUSH: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_PUSH);
    pub const APPLICATION_VND_OMA_SCIDM_MESSAGES_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_SCIDM_MESSAGES_XML);
    pub const APPLICATION_VND_OMA_XCAP_DIRECTORY_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMA_XCAP_DIRECTORY_XML);
    pub const APPLICATION_VND_OMADS_EMAIL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMADS_EMAIL_XML);
    pub const APPLICATION_VND_OMADS_FILE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMADS_FILE_XML);
    pub const APPLICATION_VND_OMADS_FOLDER_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMADS_FOLDER_XML);
    pub const APPLICATION_VND_OMALOC_SUPL_INIT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OMALOC_SUPL_INIT);
    pub const APPLICATION_VND_ONEPAGER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ONEPAGER);
    pub const APPLICATION_VND_ONEPAGERTAMP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ONEPAGERTAMP);
    pub const APPLICATION_VND_ONEPAGERTAMX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ONEPAGERTAMX);
    pub const APPLICATION_VND_ONEPAGERTAT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ONEPAGERTAT);
    pub const APPLICATION_VND_ONEPAGERTATP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ONEPAGERTATP);
    pub const APPLICATION_VND_ONEPAGERTATX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ONEPAGERTATX);
    pub const APPLICATION_VND_ONVIF_METADATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ONVIF_METADATA);
    pub const APPLICATION_VND_OPENBLOX_GAME_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENBLOX_GAME_XML);
    pub const APPLICATION_VND_OPENBLOX_GAME_BINARY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENBLOX_GAME_BINARY);
    pub const APPLICATION_VND_OPENEYE_OEB: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENEYE_OEB);
    pub const APPLICATION_VND_OPENSTREETMAP_DATA_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENSTREETMAP_DATA_XML);
    pub const APPLICATION_VND_OPENTIMESTAMPS_OTS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENTIMESTAMPS_OTS);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOM_PROPERTIES_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOM_PROPERTIES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOMXMLPROPERTIES_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_CUSTOMXMLPROPERTIES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWING_XML: Encoding = Encoding::new(
        IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWING_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHART_XML: Encoding =
        Encoding::new(
            IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHART_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHARTSHAPES_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_CHARTSHAPES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMCOLORS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMCOLORS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMDATA_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMDATA_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMLAYOUT_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMLAYOUT_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMSTYLE_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_DRAWINGML_DIAGRAMSTYLE_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_EXTENDED_PROPERTIES_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_EXTENDED_PROPERTIES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTAUTHORS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTAUTHORS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_COMMENTS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_HANDOUTMASTER_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_HANDOUTMASTER_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESMASTER_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESMASTER_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESSLIDE_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_NOTESSLIDE_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESPROPS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESPROPS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION_MAIN_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_PRESENTATION_MAIN_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE: Encoding =
        Encoding::new(
            IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDE_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDELAYOUT_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDELAYOUT_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEMASTER_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEMASTER_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEUPDATEINFO_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDEUPDATEINFO_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW_MAIN_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_SLIDESHOW_MAIN_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TABLESTYLES_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TABLESTYLES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TAGS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TAGS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE_MAIN_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_TEMPLATE_MAIN_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_VIEWPROPS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_PRESENTATIONML_VIEWPROPS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CALCCHAIN_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CALCCHAIN_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CHARTSHEET_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CHARTSHEET_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_COMMENTS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_COMMENTS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CONNECTIONS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_CONNECTIONS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_DIALOGSHEET_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_DIALOGSHEET_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_EXTERNALLINK_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_EXTERNALLINK_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHEDEFINITION_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHEDEFINITION_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHERECORDS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTCACHERECORDS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTTABLE_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_PIVOTTABLE_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_QUERYTABLE_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_QUERYTABLE_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONHEADERS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONHEADERS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONLOG_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_REVISIONLOG_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHAREDSTRINGS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHAREDSTRINGS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET: Encoding =
        Encoding::new(
            IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET_MAIN_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEET_MAIN_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEETMETADATA_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_SHEETMETADATA_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_STYLES_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_STYLES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLE_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLE_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLESINGLECELLS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TABLESINGLECELLS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE_MAIN_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_TEMPLATE_MAIN_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_USERNAMES_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_USERNAMES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_VOLATILEDEPENDENCIES_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_VOLATILEDEPENDENCIES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_WORKSHEET_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_SPREADSHEETML_WORKSHEET_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEME_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEME_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEMEOVERRIDE_XML: Encoding =
        Encoding::new(
            IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_THEMEOVERRIDE_XML,
        );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_VMLDRAWING: Encoding = Encoding::new(
        IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_VMLDRAWING,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_COMMENTS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_COMMENTS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_GLOSSARY_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_GLOSSARY_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_MAIN_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_DOCUMENT_MAIN_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_ENDNOTES_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_ENDNOTES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FONTTABLE_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FONTTABLE_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTER_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTER_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTNOTES_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_FOOTNOTES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_NUMBERING_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_NUMBERING_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_SETTINGS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_SETTINGS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_STYLES_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_STYLES_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE_MAIN_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_TEMPLATE_MAIN_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_WEBSETTINGS_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_OFFICEDOCUMENT_WORDPROCESSINGML_WEBSETTINGS_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_CORE_PROPERTIES_XML: Encoding = Encoding::new(
        IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_PACKAGE_CORE_PROPERTIES_XML,
    );
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_DIGITAL_SIGNATURE_XMLSIGNATURE_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_PACKAGE_DIGITAL_SIGNATURE_XMLSIGNATURE_XML);
    pub const APPLICATION_VND_OPENXMLFORMATS_PACKAGE_RELATIONSHIPS_XML: Encoding = Encoding::new(
        IanaEncodingMapping::APPLICATION_VND_OPENXMLFORMATS_PACKAGE_RELATIONSHIPS_XML,
    );
    pub const APPLICATION_VND_ORACLE_RESOURCE_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ORACLE_RESOURCE_JSON);
    pub const APPLICATION_VND_ORANGE_INDATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ORANGE_INDATA);
    pub const APPLICATION_VND_OSA_NETDEPLOY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OSA_NETDEPLOY);
    pub const APPLICATION_VND_OSGEO_MAPGUIDE_PACKAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OSGEO_MAPGUIDE_PACKAGE);
    pub const APPLICATION_VND_OSGI_BUNDLE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OSGI_BUNDLE);
    pub const APPLICATION_VND_OSGI_DP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OSGI_DP);
    pub const APPLICATION_VND_OSGI_SUBSYSTEM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OSGI_SUBSYSTEM);
    pub const APPLICATION_VND_OTPS_CT_KIP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OTPS_CT_KIP_XML);
    pub const APPLICATION_VND_OXLI_COUNTGRAPH: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_OXLI_COUNTGRAPH);
    pub const APPLICATION_VND_PAGERDUTY_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PAGERDUTY_JSON);
    pub const APPLICATION_VND_PALM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PALM);
    pub const APPLICATION_VND_PANOPLY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PANOPLY);
    pub const APPLICATION_VND_PAOS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PAOS_XML);
    pub const APPLICATION_VND_PATENTDIVE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PATENTDIVE);
    pub const APPLICATION_VND_PATIENTECOMMSDOC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PATIENTECOMMSDOC);
    pub const APPLICATION_VND_PAWAAFILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PAWAAFILE);
    pub const APPLICATION_VND_PCOS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PCOS);
    pub const APPLICATION_VND_PG_FORMAT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PG_FORMAT);
    pub const APPLICATION_VND_PG_OSASLI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PG_OSASLI);
    pub const APPLICATION_VND_PIACCESS_APPLICATION_LICENCE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PIACCESS_APPLICATION_LICENCE);
    pub const APPLICATION_VND_PICSEL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PICSEL);
    pub const APPLICATION_VND_PMI_WIDGET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PMI_WIDGET);
    pub const APPLICATION_VND_POC_GROUP_ADVERTISEMENT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_POC_GROUP_ADVERTISEMENT_XML);
    pub const APPLICATION_VND_POCKETLEARN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_POCKETLEARN);
    pub const APPLICATION_VND_POWERBUILDER6: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_POWERBUILDER6);
    pub const APPLICATION_VND_POWERBUILDER6_S: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_POWERBUILDER6_S);
    pub const APPLICATION_VND_POWERBUILDER7: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_POWERBUILDER7);
    pub const APPLICATION_VND_POWERBUILDER7_S: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_POWERBUILDER7_S);
    pub const APPLICATION_VND_POWERBUILDER75: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_POWERBUILDER75);
    pub const APPLICATION_VND_POWERBUILDER75_S: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_POWERBUILDER75_S);
    pub const APPLICATION_VND_PREMINET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PREMINET);
    pub const APPLICATION_VND_PREVIEWSYSTEMS_BOX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PREVIEWSYSTEMS_BOX);
    pub const APPLICATION_VND_PROTEUS_MAGAZINE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PROTEUS_MAGAZINE);
    pub const APPLICATION_VND_PSFS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PSFS);
    pub const APPLICATION_VND_PT_MUNDUSMUNDI: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PT_MUNDUSMUNDI);
    pub const APPLICATION_VND_PUBLISHARE_DELTA_TREE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PUBLISHARE_DELTA_TREE);
    pub const APPLICATION_VND_PVI_PTID1: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PVI_PTID1);
    pub const APPLICATION_VND_PWG_MULTIPLEXED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PWG_MULTIPLEXED);
    pub const APPLICATION_VND_PWG_XHTML_PRINT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_PWG_XHTML_PRINT_XML);
    pub const APPLICATION_VND_QUALCOMM_BREW_APP_RES: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_QUALCOMM_BREW_APP_RES);
    pub const APPLICATION_VND_QUARANTAINENET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_QUARANTAINENET);
    pub const APPLICATION_VND_QUOBJECT_QUOXDOCUMENT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_QUOBJECT_QUOXDOCUMENT);
    pub const APPLICATION_VND_RADISYS_MOML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MOML_XML);
    pub const APPLICATION_VND_RADISYS_MSML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_XML);
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_AUDIT_XML);
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_CONF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_AUDIT_CONF_XML);
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_CONN_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_AUDIT_CONN_XML);
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_DIALOG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_AUDIT_DIALOG_XML);
    pub const APPLICATION_VND_RADISYS_MSML_AUDIT_STREAM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_AUDIT_STREAM_XML);
    pub const APPLICATION_VND_RADISYS_MSML_CONF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_CONF_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_DIALOG_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_BASE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_DIALOG_BASE_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_DETECT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_DETECT_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_SENDRECV_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_DIALOG_FAX_SENDRECV_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_GROUP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_DIALOG_GROUP_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_SPEECH_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_DIALOG_SPEECH_XML);
    pub const APPLICATION_VND_RADISYS_MSML_DIALOG_TRANSFORM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RADISYS_MSML_DIALOG_TRANSFORM_XML);
    pub const APPLICATION_VND_RAINSTOR_DATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RAINSTOR_DATA);
    pub const APPLICATION_VND_RAPID: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RAPID);
    pub const APPLICATION_VND_RAR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RAR);
    pub const APPLICATION_VND_REALVNC_BED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_REALVNC_BED);
    pub const APPLICATION_VND_RECORDARE_MUSICXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RECORDARE_MUSICXML);
    pub const APPLICATION_VND_RECORDARE_MUSICXML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RECORDARE_MUSICXML_XML);
    pub const APPLICATION_VND_RELPIPE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RELPIPE);
    pub const APPLICATION_VND_RESILIENT_LOGIC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RESILIENT_LOGIC);
    pub const APPLICATION_VND_RESTFUL_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RESTFUL_JSON);
    pub const APPLICATION_VND_RIG_CRYPTONOTE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RIG_CRYPTONOTE);
    pub const APPLICATION_VND_ROUTE66_LINK66_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ROUTE66_LINK66_XML);
    pub const APPLICATION_VND_RS_274X: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RS_274X);
    pub const APPLICATION_VND_RUCKUS_DOWNLOAD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_RUCKUS_DOWNLOAD);
    pub const APPLICATION_VND_S3SMS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_S3SMS);
    pub const APPLICATION_VND_SAILINGTRACKER_TRACK: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SAILINGTRACKER_TRACK);
    pub const APPLICATION_VND_SAR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SAR);
    pub const APPLICATION_VND_SBM_CID: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SBM_CID);
    pub const APPLICATION_VND_SBM_MID2: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SBM_MID2);
    pub const APPLICATION_VND_SCRIBUS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SCRIBUS);
    pub const APPLICATION_VND_SEALED_3DF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEALED_3DF);
    pub const APPLICATION_VND_SEALED_CSF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEALED_CSF);
    pub const APPLICATION_VND_SEALED_DOC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEALED_DOC);
    pub const APPLICATION_VND_SEALED_EML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEALED_EML);
    pub const APPLICATION_VND_SEALED_MHT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEALED_MHT);
    pub const APPLICATION_VND_SEALED_NET: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEALED_NET);
    pub const APPLICATION_VND_SEALED_PPT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEALED_PPT);
    pub const APPLICATION_VND_SEALED_TIFF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEALED_TIFF);
    pub const APPLICATION_VND_SEALED_XLS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEALED_XLS);
    pub const APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_HTML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_HTML);
    pub const APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_PDF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEALEDMEDIA_SOFTSEAL_PDF);
    pub const APPLICATION_VND_SEEMAIL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEEMAIL);
    pub const APPLICATION_VND_SEIS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEIS_JSON);
    pub const APPLICATION_VND_SEMA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEMA);
    pub const APPLICATION_VND_SEMD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEMD);
    pub const APPLICATION_VND_SEMF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SEMF);
    pub const APPLICATION_VND_SHADE_SAVE_FILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SHADE_SAVE_FILE);
    pub const APPLICATION_VND_SHANA_INFORMED_FORMDATA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SHANA_INFORMED_FORMDATA);
    pub const APPLICATION_VND_SHANA_INFORMED_FORMTEMPLATE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SHANA_INFORMED_FORMTEMPLATE);
    pub const APPLICATION_VND_SHANA_INFORMED_INTERCHANGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SHANA_INFORMED_INTERCHANGE);
    pub const APPLICATION_VND_SHANA_INFORMED_PACKAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SHANA_INFORMED_PACKAGE);
    pub const APPLICATION_VND_SHOOTPROOF_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SHOOTPROOF_JSON);
    pub const APPLICATION_VND_SHOPKICK_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SHOPKICK_JSON);
    pub const APPLICATION_VND_SHP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SHP);
    pub const APPLICATION_VND_SHX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SHX);
    pub const APPLICATION_VND_SIGROK_SESSION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SIGROK_SESSION);
    pub const APPLICATION_VND_SIREN_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SIREN_JSON);
    pub const APPLICATION_VND_SMAF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SMAF);
    pub const APPLICATION_VND_SMART_NOTEBOOK: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SMART_NOTEBOOK);
    pub const APPLICATION_VND_SMART_TEACHER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SMART_TEACHER);
    pub const APPLICATION_VND_SMINTIO_PORTALS_ARCHIVE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SMINTIO_PORTALS_ARCHIVE);
    pub const APPLICATION_VND_SNESDEV_PAGE_TABLE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SNESDEV_PAGE_TABLE);
    pub const APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML);
    pub const APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML_ZIP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SOFTWARE602_FILLER_FORM_XML_ZIP);
    pub const APPLICATION_VND_SOLENT_SDKM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SOLENT_SDKM_XML);
    pub const APPLICATION_VND_SPOTFIRE_DXP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SPOTFIRE_DXP);
    pub const APPLICATION_VND_SPOTFIRE_SFS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SPOTFIRE_SFS);
    pub const APPLICATION_VND_SQLITE3: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SQLITE3);
    pub const APPLICATION_VND_SSS_COD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SSS_COD);
    pub const APPLICATION_VND_SSS_DTF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SSS_DTF);
    pub const APPLICATION_VND_SSS_NTF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SSS_NTF);
    pub const APPLICATION_VND_STEPMANIA_PACKAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_STEPMANIA_PACKAGE);
    pub const APPLICATION_VND_STEPMANIA_STEPCHART: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_STEPMANIA_STEPCHART);
    pub const APPLICATION_VND_STREET_STREAM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_STREET_STREAM);
    pub const APPLICATION_VND_SUN_WADL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SUN_WADL_XML);
    pub const APPLICATION_VND_SUS_CALENDAR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SUS_CALENDAR);
    pub const APPLICATION_VND_SVD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SVD);
    pub const APPLICATION_VND_SWIFTVIEW_ICS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SWIFTVIEW_ICS);
    pub const APPLICATION_VND_SYBYL_MOL2: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYBYL_MOL2);
    pub const APPLICATION_VND_SYCLE_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYCLE_XML);
    pub const APPLICATION_VND_SYFT_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYFT_JSON);
    pub const APPLICATION_VND_SYNCML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYNCML_XML);
    pub const APPLICATION_VND_SYNCML_DM_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYNCML_DM_WBXML);
    pub const APPLICATION_VND_SYNCML_DM_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYNCML_DM_XML);
    pub const APPLICATION_VND_SYNCML_DM_NOTIFICATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYNCML_DM_NOTIFICATION);
    pub const APPLICATION_VND_SYNCML_DMDDF_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYNCML_DMDDF_WBXML);
    pub const APPLICATION_VND_SYNCML_DMDDF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYNCML_DMDDF_XML);
    pub const APPLICATION_VND_SYNCML_DMTNDS_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYNCML_DMTNDS_WBXML);
    pub const APPLICATION_VND_SYNCML_DMTNDS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYNCML_DMTNDS_XML);
    pub const APPLICATION_VND_SYNCML_DS_NOTIFICATION: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_SYNCML_DS_NOTIFICATION);
    pub const APPLICATION_VND_TABLESCHEMA_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_TABLESCHEMA_JSON);
    pub const APPLICATION_VND_TAO_INTENT_MODULE_ARCHIVE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_TAO_INTENT_MODULE_ARCHIVE);
    pub const APPLICATION_VND_TCPDUMP_PCAP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_TCPDUMP_PCAP);
    pub const APPLICATION_VND_THINK_CELL_PPTTC_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_THINK_CELL_PPTTC_JSON);
    pub const APPLICATION_VND_TMD_MEDIAFLEX_API_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_TMD_MEDIAFLEX_API_XML);
    pub const APPLICATION_VND_TML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_TML);
    pub const APPLICATION_VND_TMOBILE_LIVETV: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_TMOBILE_LIVETV);
    pub const APPLICATION_VND_TRI_ONESOURCE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_TRI_ONESOURCE);
    pub const APPLICATION_VND_TRID_TPT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_TRID_TPT);
    pub const APPLICATION_VND_TRISCAPE_MXS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_TRISCAPE_MXS);
    pub const APPLICATION_VND_TRUEAPP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_TRUEAPP);
    pub const APPLICATION_VND_TRUEDOC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_TRUEDOC);
    pub const APPLICATION_VND_UBISOFT_WEBPLAYER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UBISOFT_WEBPLAYER);
    pub const APPLICATION_VND_UFDL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UFDL);
    pub const APPLICATION_VND_UIQ_THEME: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UIQ_THEME);
    pub const APPLICATION_VND_UMAJIN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UMAJIN);
    pub const APPLICATION_VND_UNITY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UNITY);
    pub const APPLICATION_VND_UOML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UOML_XML);
    pub const APPLICATION_VND_UPLANET_ALERT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_ALERT);
    pub const APPLICATION_VND_UPLANET_ALERT_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_ALERT_WBXML);
    pub const APPLICATION_VND_UPLANET_BEARER_CHOICE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_BEARER_CHOICE);
    pub const APPLICATION_VND_UPLANET_BEARER_CHOICE_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_BEARER_CHOICE_WBXML);
    pub const APPLICATION_VND_UPLANET_CACHEOP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_CACHEOP);
    pub const APPLICATION_VND_UPLANET_CACHEOP_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_CACHEOP_WBXML);
    pub const APPLICATION_VND_UPLANET_CHANNEL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_CHANNEL);
    pub const APPLICATION_VND_UPLANET_CHANNEL_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_CHANNEL_WBXML);
    pub const APPLICATION_VND_UPLANET_LIST: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_LIST);
    pub const APPLICATION_VND_UPLANET_LIST_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_LIST_WBXML);
    pub const APPLICATION_VND_UPLANET_LISTCMD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_LISTCMD);
    pub const APPLICATION_VND_UPLANET_LISTCMD_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_LISTCMD_WBXML);
    pub const APPLICATION_VND_UPLANET_SIGNAL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_UPLANET_SIGNAL);
    pub const APPLICATION_VND_URI_MAP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_URI_MAP);
    pub const APPLICATION_VND_VALVE_SOURCE_MATERIAL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VALVE_SOURCE_MATERIAL);
    pub const APPLICATION_VND_VCX: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VCX);
    pub const APPLICATION_VND_VD_STUDY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VD_STUDY);
    pub const APPLICATION_VND_VECTORWORKS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VECTORWORKS);
    pub const APPLICATION_VND_VEL_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VEL_JSON);
    pub const APPLICATION_VND_VERIMATRIX_VCAS: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VERIMATRIX_VCAS);
    pub const APPLICATION_VND_VERITONE_AION_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VERITONE_AION_JSON);
    pub const APPLICATION_VND_VERYANT_THIN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VERYANT_THIN);
    pub const APPLICATION_VND_VES_ENCRYPTED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VES_ENCRYPTED);
    pub const APPLICATION_VND_VIDSOFT_VIDCONFERENCE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VIDSOFT_VIDCONFERENCE);
    pub const APPLICATION_VND_VISIO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VISIO);
    pub const APPLICATION_VND_VISIONARY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VISIONARY);
    pub const APPLICATION_VND_VIVIDENCE_SCRIPTFILE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VIVIDENCE_SCRIPTFILE);
    pub const APPLICATION_VND_VSF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_VSF);
    pub const APPLICATION_VND_WAP_SIC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WAP_SIC);
    pub const APPLICATION_VND_WAP_SLC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WAP_SLC);
    pub const APPLICATION_VND_WAP_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WAP_WBXML);
    pub const APPLICATION_VND_WAP_WMLC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WAP_WMLC);
    pub const APPLICATION_VND_WAP_WMLSCRIPTC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WAP_WMLSCRIPTC);
    pub const APPLICATION_VND_WASMFLOW_WAFL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WASMFLOW_WAFL);
    pub const APPLICATION_VND_WEBTURBO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WEBTURBO);
    pub const APPLICATION_VND_WFA_DPP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WFA_DPP);
    pub const APPLICATION_VND_WFA_P2P: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WFA_P2P);
    pub const APPLICATION_VND_WFA_WSC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WFA_WSC);
    pub const APPLICATION_VND_WINDOWS_DEVICEPAIRING: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WINDOWS_DEVICEPAIRING);
    pub const APPLICATION_VND_WMC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WMC);
    pub const APPLICATION_VND_WMF_BOOTSTRAP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WMF_BOOTSTRAP);
    pub const APPLICATION_VND_WOLFRAM_MATHEMATICA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WOLFRAM_MATHEMATICA);
    pub const APPLICATION_VND_WOLFRAM_MATHEMATICA_PACKAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WOLFRAM_MATHEMATICA_PACKAGE);
    pub const APPLICATION_VND_WOLFRAM_PLAYER: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WOLFRAM_PLAYER);
    pub const APPLICATION_VND_WORDLIFT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WORDLIFT);
    pub const APPLICATION_VND_WORDPERFECT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WORDPERFECT);
    pub const APPLICATION_VND_WQD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WQD);
    pub const APPLICATION_VND_WRQ_HP3000_LABELLED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WRQ_HP3000_LABELLED);
    pub const APPLICATION_VND_WT_STF: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WT_STF);
    pub const APPLICATION_VND_WV_CSP_WBXML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WV_CSP_WBXML);
    pub const APPLICATION_VND_WV_CSP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WV_CSP_XML);
    pub const APPLICATION_VND_WV_SSP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_WV_SSP_XML);
    pub const APPLICATION_VND_XACML_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_XACML_JSON);
    pub const APPLICATION_VND_XARA: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_XARA);
    pub const APPLICATION_VND_XECRETS_ENCRYPTED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_XECRETS_ENCRYPTED);
    pub const APPLICATION_VND_XFDL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_XFDL);
    pub const APPLICATION_VND_XFDL_WEBFORM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_XFDL_WEBFORM);
    pub const APPLICATION_VND_XMI_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_XMI_XML);
    pub const APPLICATION_VND_XMPIE_CPKG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_XMPIE_CPKG);
    pub const APPLICATION_VND_XMPIE_DPKG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_XMPIE_DPKG);
    pub const APPLICATION_VND_XMPIE_PLAN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_XMPIE_PLAN);
    pub const APPLICATION_VND_XMPIE_PPKG: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_XMPIE_PPKG);
    pub const APPLICATION_VND_XMPIE_XLIM: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_XMPIE_XLIM);
    pub const APPLICATION_VND_YAMAHA_HV_DIC: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YAMAHA_HV_DIC);
    pub const APPLICATION_VND_YAMAHA_HV_SCRIPT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YAMAHA_HV_SCRIPT);
    pub const APPLICATION_VND_YAMAHA_HV_VOICE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YAMAHA_HV_VOICE);
    pub const APPLICATION_VND_YAMAHA_OPENSCOREFORMAT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YAMAHA_OPENSCOREFORMAT);
    pub const APPLICATION_VND_YAMAHA_OPENSCOREFORMAT_OSFPVG_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YAMAHA_OPENSCOREFORMAT_OSFPVG_XML);
    pub const APPLICATION_VND_YAMAHA_REMOTE_SETUP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YAMAHA_REMOTE_SETUP);
    pub const APPLICATION_VND_YAMAHA_SMAF_AUDIO: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YAMAHA_SMAF_AUDIO);
    pub const APPLICATION_VND_YAMAHA_SMAF_PHRASE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YAMAHA_SMAF_PHRASE);
    pub const APPLICATION_VND_YAMAHA_THROUGH_NGN: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YAMAHA_THROUGH_NGN);
    pub const APPLICATION_VND_YAMAHA_TUNNEL_UDPENCAP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YAMAHA_TUNNEL_UDPENCAP);
    pub const APPLICATION_VND_YAOWEME: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YAOWEME);
    pub const APPLICATION_VND_YELLOWRIVER_CUSTOM_MENU: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YELLOWRIVER_CUSTOM_MENU);
    pub const APPLICATION_VND_YOUTUBE_YT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_YOUTUBE_YT);
    pub const APPLICATION_VND_ZUL: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ZUL);
    pub const APPLICATION_VND_ZZAZZ_DECK_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VND_ZZAZZ_DECK_XML);
    pub const APPLICATION_VOICEXML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VOICEXML_XML);
    pub const APPLICATION_VOUCHER_CMS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VOUCHER_CMS_JSON);
    pub const APPLICATION_VQ_RTCPXR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_VQ_RTCPXR);
    pub const APPLICATION_WASM: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_WASM);
    pub const APPLICATION_WATCHERINFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_WATCHERINFO_XML);
    pub const APPLICATION_WEBPUSH_OPTIONS_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_WEBPUSH_OPTIONS_JSON);
    pub const APPLICATION_WHOISPP_QUERY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_WHOISPP_QUERY);
    pub const APPLICATION_WHOISPP_RESPONSE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_WHOISPP_RESPONSE);
    pub const APPLICATION_WIDGET: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_WIDGET);
    pub const APPLICATION_WITA: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_WITA);
    pub const APPLICATION_WORDPERFECT5_1: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_WORDPERFECT5_1);
    pub const APPLICATION_WSDL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_WSDL_XML);
    pub const APPLICATION_WSPOLICY_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_WSPOLICY_XML);
    pub const APPLICATION_X_PKI_MESSAGE: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_X_PKI_MESSAGE);
    pub const APPLICATION_X_WWW_FORM_URLENCODED: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_X_WWW_FORM_URLENCODED);
    pub const APPLICATION_X_X509_CA_CERT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_X_X509_CA_CERT);
    pub const APPLICATION_X_X509_CA_RA_CERT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_X_X509_CA_RA_CERT);
    pub const APPLICATION_X_X509_NEXT_CA_CERT: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_X_X509_NEXT_CA_CERT);
    pub const APPLICATION_X400_BP: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_X400_BP);
    pub const APPLICATION_XACML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XACML_XML);
    pub const APPLICATION_XCAP_ATT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XCAP_ATT_XML);
    pub const APPLICATION_XCAP_CAPS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XCAP_CAPS_XML);
    pub const APPLICATION_XCAP_DIFF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XCAP_DIFF_XML);
    pub const APPLICATION_XCAP_EL_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XCAP_EL_XML);
    pub const APPLICATION_XCAP_ERROR_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XCAP_ERROR_XML);
    pub const APPLICATION_XCAP_NS_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XCAP_NS_XML);
    pub const APPLICATION_XCON_CONFERENCE_INFO_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XCON_CONFERENCE_INFO_XML);
    pub const APPLICATION_XCON_CONFERENCE_INFO_DIFF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XCON_CONFERENCE_INFO_DIFF_XML);
    pub const APPLICATION_XENC_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XENC_XML);
    pub const APPLICATION_XFDF: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_XFDF);
    pub const APPLICATION_XHTML_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XHTML_XML);
    pub const APPLICATION_XLIFF_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XLIFF_XML);
    pub const APPLICATION_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_XML);
    pub const APPLICATION_XML_DTD: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XML_DTD);
    pub const APPLICATION_XML_EXTERNAL_PARSED_ENTITY: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XML_EXTERNAL_PARSED_ENTITY);
    pub const APPLICATION_XML_PATCH_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XML_PATCH_XML);
    pub const APPLICATION_XMPP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XMPP_XML);
    pub const APPLICATION_XOP_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XOP_XML);
    pub const APPLICATION_XSLT_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_XSLT_XML);
    pub const APPLICATION_XV_XML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_XV_XML);
    pub const APPLICATION_YAML: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_YAML);
    pub const APPLICATION_YANG: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_YANG);
    pub const APPLICATION_YANG_DATA_CBOR: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_YANG_DATA_CBOR);
    pub const APPLICATION_YANG_DATA_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_YANG_DATA_JSON);
    pub const APPLICATION_YANG_DATA_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_YANG_DATA_XML);
    pub const APPLICATION_YANG_PATCH_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_YANG_PATCH_JSON);
    pub const APPLICATION_YANG_PATCH_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_YANG_PATCH_XML);
    pub const APPLICATION_YANG_SID_JSON: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_YANG_SID_JSON);
    pub const APPLICATION_YIN_XML: Encoding =
        Encoding::new(IanaEncodingMapping::APPLICATION_YIN_XML);
    pub const APPLICATION_ZIP: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_ZIP);
    pub const APPLICATION_ZLIB: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_ZLIB);
    pub const APPLICATION_ZSTD: Encoding = Encoding::new(IanaEncodingMapping::APPLICATION_ZSTD);
    pub const AUDIO_1D_INTERLEAVED_PARITYFEC: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_1D_INTERLEAVED_PARITYFEC);
    pub const AUDIO_32KADPCM: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_32KADPCM);
    pub const AUDIO_3GPP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_3GPP);
    pub const AUDIO_3GPP2: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_3GPP2);
    pub const AUDIO_AMR: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_AMR);
    pub const AUDIO_AMR_WB: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_AMR_WB);
    pub const AUDIO_ATRAC_ADVANCED_LOSSLESS: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_ATRAC_ADVANCED_LOSSLESS);
    pub const AUDIO_ATRAC_X: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_ATRAC_X);
    pub const AUDIO_ATRAC3: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_ATRAC3);
    pub const AUDIO_BV16: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_BV16);
    pub const AUDIO_BV32: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_BV32);
    pub const AUDIO_CN: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_CN);
    pub const AUDIO_DAT12: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_DAT12);
    pub const AUDIO_DV: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_DV);
    pub const AUDIO_DVI4: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_DVI4);
    pub const AUDIO_EVRC: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRC);
    pub const AUDIO_EVRC_QCP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRC_QCP);
    pub const AUDIO_EVRC0: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRC0);
    pub const AUDIO_EVRC1: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRC1);
    pub const AUDIO_EVRCB: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRCB);
    pub const AUDIO_EVRCB0: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRCB0);
    pub const AUDIO_EVRCB1: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRCB1);
    pub const AUDIO_EVRCNW: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRCNW);
    pub const AUDIO_EVRCNW0: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRCNW0);
    pub const AUDIO_EVRCNW1: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRCNW1);
    pub const AUDIO_EVRCWB: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRCWB);
    pub const AUDIO_EVRCWB0: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRCWB0);
    pub const AUDIO_EVRCWB1: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVRCWB1);
    pub const AUDIO_EVS: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EVS);
    pub const AUDIO_G711_0: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G711_0);
    pub const AUDIO_G719: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G719);
    pub const AUDIO_G722: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G722);
    pub const AUDIO_G7221: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G7221);
    pub const AUDIO_G723: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G723);
    pub const AUDIO_G726_16: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G726_16);
    pub const AUDIO_G726_24: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G726_24);
    pub const AUDIO_G726_32: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G726_32);
    pub const AUDIO_G726_40: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G726_40);
    pub const AUDIO_G728: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G728);
    pub const AUDIO_G729: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G729);
    pub const AUDIO_G7291: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G7291);
    pub const AUDIO_G729D: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G729D);
    pub const AUDIO_G729E: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_G729E);
    pub const AUDIO_GSM: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_GSM);
    pub const AUDIO_GSM_EFR: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_GSM_EFR);
    pub const AUDIO_GSM_HR_08: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_GSM_HR_08);
    pub const AUDIO_L16: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_L16);
    pub const AUDIO_L20: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_L20);
    pub const AUDIO_L24: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_L24);
    pub const AUDIO_L8: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_L8);
    pub const AUDIO_LPC: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_LPC);
    pub const AUDIO_MELP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MELP);
    pub const AUDIO_MELP1200: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MELP1200);
    pub const AUDIO_MELP2400: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MELP2400);
    pub const AUDIO_MELP600: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MELP600);
    pub const AUDIO_MP4A_LATM: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MP4A_LATM);
    pub const AUDIO_MPA: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MPA);
    pub const AUDIO_PCMA: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_PCMA);
    pub const AUDIO_PCMA_WB: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_PCMA_WB);
    pub const AUDIO_PCMU: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_PCMU);
    pub const AUDIO_PCMU_WB: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_PCMU_WB);
    pub const AUDIO_QCELP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_QCELP);
    pub const AUDIO_RED: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_RED);
    pub const AUDIO_SMV: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_SMV);
    pub const AUDIO_SMV_QCP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_SMV_QCP);
    pub const AUDIO_SMV0: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_SMV0);
    pub const AUDIO_TETRA_ACELP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_TETRA_ACELP);
    pub const AUDIO_TETRA_ACELP_BB: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_TETRA_ACELP_BB);
    pub const AUDIO_TSVCIS: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_TSVCIS);
    pub const AUDIO_UEMCLIP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_UEMCLIP);
    pub const AUDIO_VDVI: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VDVI);
    pub const AUDIO_VMR_WB: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VMR_WB);
    pub const AUDIO_AAC: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_AAC);
    pub const AUDIO_AC3: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_AC3);
    pub const AUDIO_AMR_WB_P: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_AMR_WB_P);
    pub const AUDIO_APTX: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_APTX);
    pub const AUDIO_ASC: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_ASC);
    pub const AUDIO_BASIC: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_BASIC);
    pub const AUDIO_CLEARMODE: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_CLEARMODE);
    pub const AUDIO_DLS: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_DLS);
    pub const AUDIO_DSR_ES201108: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_DSR_ES201108);
    pub const AUDIO_DSR_ES202050: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_DSR_ES202050);
    pub const AUDIO_DSR_ES202211: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_DSR_ES202211);
    pub const AUDIO_DSR_ES202212: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_DSR_ES202212);
    pub const AUDIO_EAC3: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EAC3);
    pub const AUDIO_ENCAPRTP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_ENCAPRTP);
    pub const AUDIO_EXAMPLE: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_EXAMPLE);
    pub const AUDIO_FLEXFEC: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_FLEXFEC);
    pub const AUDIO_FWDRED: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_FWDRED);
    pub const AUDIO_ILBC: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_ILBC);
    pub const AUDIO_IP_MR_V2_5: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_IP_MR_V2_5);
    pub const AUDIO_MATROSKA: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MATROSKA);
    pub const AUDIO_MHAS: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MHAS);
    pub const AUDIO_MOBILE_XMF: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MOBILE_XMF);
    pub const AUDIO_MP4: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MP4);
    pub const AUDIO_MPA_ROBUST: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MPA_ROBUST);
    pub const AUDIO_MPEG: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_MPEG);
    pub const AUDIO_MPEG4_GENERIC: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_MPEG4_GENERIC);
    pub const AUDIO_OGG: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_OGG);
    pub const AUDIO_OPUS: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_OPUS);
    pub const AUDIO_PARITYFEC: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_PARITYFEC);
    pub const AUDIO_PRS_SID: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_PRS_SID);
    pub const AUDIO_RAPTORFEC: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_RAPTORFEC);
    pub const AUDIO_RTP_ENC_AESCM128: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_RTP_ENC_AESCM128);
    pub const AUDIO_RTP_MIDI: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_RTP_MIDI);
    pub const AUDIO_RTPLOOPBACK: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_RTPLOOPBACK);
    pub const AUDIO_RTX: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_RTX);
    pub const AUDIO_SCIP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_SCIP);
    pub const AUDIO_SOFA: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_SOFA);
    pub const AUDIO_SP_MIDI: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_SP_MIDI);
    pub const AUDIO_SPEEX: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_SPEEX);
    pub const AUDIO_T140C: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_T140C);
    pub const AUDIO_T38: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_T38);
    pub const AUDIO_TELEPHONE_EVENT: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_TELEPHONE_EVENT);
    pub const AUDIO_TONE: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_TONE);
    pub const AUDIO_ULPFEC: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_ULPFEC);
    pub const AUDIO_USAC: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_USAC);
    pub const AUDIO_VND_3GPP_IUFP: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_3GPP_IUFP);
    pub const AUDIO_VND_4SB: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_4SB);
    pub const AUDIO_VND_CELP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_CELP);
    pub const AUDIO_VND_AUDIOKOZ: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_AUDIOKOZ);
    pub const AUDIO_VND_CISCO_NSE: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_CISCO_NSE);
    pub const AUDIO_VND_CMLES_RADIO_EVENTS: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_CMLES_RADIO_EVENTS);
    pub const AUDIO_VND_CNS_ANP1: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_CNS_ANP1);
    pub const AUDIO_VND_CNS_INF1: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_CNS_INF1);
    pub const AUDIO_VND_DECE_AUDIO: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_DECE_AUDIO);
    pub const AUDIO_VND_DIGITAL_WINDS: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_DIGITAL_WINDS);
    pub const AUDIO_VND_DLNA_ADTS: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_DLNA_ADTS);
    pub const AUDIO_VND_DOLBY_HEAAC_1: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_DOLBY_HEAAC_1);
    pub const AUDIO_VND_DOLBY_HEAAC_2: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_DOLBY_HEAAC_2);
    pub const AUDIO_VND_DOLBY_MLP: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_DOLBY_MLP);
    pub const AUDIO_VND_DOLBY_MPS: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_DOLBY_MPS);
    pub const AUDIO_VND_DOLBY_PL2: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_DOLBY_PL2);
    pub const AUDIO_VND_DOLBY_PL2X: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_DOLBY_PL2X);
    pub const AUDIO_VND_DOLBY_PL2Z: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_DOLBY_PL2Z);
    pub const AUDIO_VND_DOLBY_PULSE_1: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_DOLBY_PULSE_1);
    pub const AUDIO_VND_DRA: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_DRA);
    pub const AUDIO_VND_DTS: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_DTS);
    pub const AUDIO_VND_DTS_HD: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_DTS_HD);
    pub const AUDIO_VND_DTS_UHD: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_DTS_UHD);
    pub const AUDIO_VND_DVB_FILE: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_DVB_FILE);
    pub const AUDIO_VND_EVERAD_PLJ: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_EVERAD_PLJ);
    pub const AUDIO_VND_HNS_AUDIO: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_HNS_AUDIO);
    pub const AUDIO_VND_LUCENT_VOICE: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_LUCENT_VOICE);
    pub const AUDIO_VND_MS_PLAYREADY_MEDIA_PYA: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_MS_PLAYREADY_MEDIA_PYA);
    pub const AUDIO_VND_NOKIA_MOBILE_XMF: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_NOKIA_MOBILE_XMF);
    pub const AUDIO_VND_NORTEL_VBK: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_NORTEL_VBK);
    pub const AUDIO_VND_NUERA_ECELP4800: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_NUERA_ECELP4800);
    pub const AUDIO_VND_NUERA_ECELP7470: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_NUERA_ECELP7470);
    pub const AUDIO_VND_NUERA_ECELP9600: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_NUERA_ECELP9600);
    pub const AUDIO_VND_OCTEL_SBC: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_OCTEL_SBC);
    pub const AUDIO_VND_PRESONUS_MULTITRACK: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_PRESONUS_MULTITRACK);
    pub const AUDIO_VND_QCELP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_QCELP);
    pub const AUDIO_VND_RHETOREX_32KADPCM: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_RHETOREX_32KADPCM);
    pub const AUDIO_VND_RIP: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_RIP);
    pub const AUDIO_VND_SEALEDMEDIA_SOFTSEAL_MPEG: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VND_SEALEDMEDIA_SOFTSEAL_MPEG);
    pub const AUDIO_VND_VMX_CVSD: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VND_VMX_CVSD);
    pub const AUDIO_VORBIS: Encoding = Encoding::new(IanaEncodingMapping::AUDIO_VORBIS);
    pub const AUDIO_VORBIS_CONFIG: Encoding =
        Encoding::new(IanaEncodingMapping::AUDIO_VORBIS_CONFIG);
    pub const FONT_COLLECTION: Encoding = Encoding::new(IanaEncodingMapping::FONT_COLLECTION);
    pub const FONT_OTF: Encoding = Encoding::new(IanaEncodingMapping::FONT_OTF);
    pub const FONT_SFNT: Encoding = Encoding::new(IanaEncodingMapping::FONT_SFNT);
    pub const FONT_TTF: Encoding = Encoding::new(IanaEncodingMapping::FONT_TTF);
    pub const FONT_WOFF: Encoding = Encoding::new(IanaEncodingMapping::FONT_WOFF);
    pub const FONT_WOFF2: Encoding = Encoding::new(IanaEncodingMapping::FONT_WOFF2);
    pub const IMAGE_ACES: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_ACES);
    pub const IMAGE_APNG: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_APNG);
    pub const IMAGE_AVCI: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_AVCI);
    pub const IMAGE_AVCS: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_AVCS);
    pub const IMAGE_AVIF: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_AVIF);
    pub const IMAGE_BMP: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_BMP);
    pub const IMAGE_CGM: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_CGM);
    pub const IMAGE_DICOM_RLE: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_DICOM_RLE);
    pub const IMAGE_DPX: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_DPX);
    pub const IMAGE_EMF: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_EMF);
    pub const IMAGE_EXAMPLE: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_EXAMPLE);
    pub const IMAGE_FITS: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_FITS);
    pub const IMAGE_G3FAX: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_G3FAX);
    pub const IMAGE_GIF: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_GIF);
    pub const IMAGE_HEIC: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_HEIC);
    pub const IMAGE_HEIC_SEQUENCE: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_HEIC_SEQUENCE);
    pub const IMAGE_HEIF: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_HEIF);
    pub const IMAGE_HEIF_SEQUENCE: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_HEIF_SEQUENCE);
    pub const IMAGE_HEJ2K: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_HEJ2K);
    pub const IMAGE_HSJ2: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_HSJ2);
    pub const IMAGE_IEF: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_IEF);
    pub const IMAGE_J2C: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_J2C);
    pub const IMAGE_JLS: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JLS);
    pub const IMAGE_JP2: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JP2);
    pub const IMAGE_JPEG: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JPEG);
    pub const IMAGE_JPH: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JPH);
    pub const IMAGE_JPHC: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JPHC);
    pub const IMAGE_JPM: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JPM);
    pub const IMAGE_JPX: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JPX);
    pub const IMAGE_JXR: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JXR);
    pub const IMAGE_JXRA: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JXRA);
    pub const IMAGE_JXRS: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JXRS);
    pub const IMAGE_JXS: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JXS);
    pub const IMAGE_JXSC: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JXSC);
    pub const IMAGE_JXSI: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JXSI);
    pub const IMAGE_JXSS: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_JXSS);
    pub const IMAGE_KTX: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_KTX);
    pub const IMAGE_KTX2: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_KTX2);
    pub const IMAGE_NAPLPS: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_NAPLPS);
    pub const IMAGE_PNG: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_PNG);
    pub const IMAGE_PRS_BTIF: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_PRS_BTIF);
    pub const IMAGE_PRS_PTI: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_PRS_PTI);
    pub const IMAGE_PWG_RASTER: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_PWG_RASTER);
    pub const IMAGE_SVG_XML: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_SVG_XML);
    pub const IMAGE_T38: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_T38);
    pub const IMAGE_TIFF: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_TIFF);
    pub const IMAGE_TIFF_FX: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_TIFF_FX);
    pub const IMAGE_VND_ADOBE_PHOTOSHOP: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_ADOBE_PHOTOSHOP);
    pub const IMAGE_VND_AIRZIP_ACCELERATOR_AZV: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_AIRZIP_ACCELERATOR_AZV);
    pub const IMAGE_VND_CNS_INF2: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_CNS_INF2);
    pub const IMAGE_VND_DECE_GRAPHIC: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_DECE_GRAPHIC);
    pub const IMAGE_VND_DJVU: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_DJVU);
    pub const IMAGE_VND_DVB_SUBTITLE: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_DVB_SUBTITLE);
    pub const IMAGE_VND_DWG: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_DWG);
    pub const IMAGE_VND_DXF: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_DXF);
    pub const IMAGE_VND_FASTBIDSHEET: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_FASTBIDSHEET);
    pub const IMAGE_VND_FPX: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_FPX);
    pub const IMAGE_VND_FST: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_FST);
    pub const IMAGE_VND_FUJIXEROX_EDMICS_MMR: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_FUJIXEROX_EDMICS_MMR);
    pub const IMAGE_VND_FUJIXEROX_EDMICS_RLC: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_FUJIXEROX_EDMICS_RLC);
    pub const IMAGE_VND_GLOBALGRAPHICS_PGB: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_GLOBALGRAPHICS_PGB);
    pub const IMAGE_VND_MICROSOFT_ICON: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_MICROSOFT_ICON);
    pub const IMAGE_VND_MIX: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_MIX);
    pub const IMAGE_VND_MOZILLA_APNG: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_MOZILLA_APNG);
    pub const IMAGE_VND_MS_MODI: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_MS_MODI);
    pub const IMAGE_VND_NET_FPX: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_NET_FPX);
    pub const IMAGE_VND_PCO_B16: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_PCO_B16);
    pub const IMAGE_VND_RADIANCE: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_RADIANCE);
    pub const IMAGE_VND_SEALED_PNG: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_SEALED_PNG);
    pub const IMAGE_VND_SEALEDMEDIA_SOFTSEAL_GIF: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_SEALEDMEDIA_SOFTSEAL_GIF);
    pub const IMAGE_VND_SEALEDMEDIA_SOFTSEAL_JPG: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_SEALEDMEDIA_SOFTSEAL_JPG);
    pub const IMAGE_VND_SVF: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_SVF);
    pub const IMAGE_VND_TENCENT_TAP: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_TENCENT_TAP);
    pub const IMAGE_VND_VALVE_SOURCE_TEXTURE: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_VALVE_SOURCE_TEXTURE);
    pub const IMAGE_VND_WAP_WBMP: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_WAP_WBMP);
    pub const IMAGE_VND_XIFF: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_VND_XIFF);
    pub const IMAGE_VND_ZBRUSH_PCX: Encoding =
        Encoding::new(IanaEncodingMapping::IMAGE_VND_ZBRUSH_PCX);
    pub const IMAGE_WEBP: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_WEBP);
    pub const IMAGE_WMF: Encoding = Encoding::new(IanaEncodingMapping::IMAGE_WMF);
    pub const MESSAGE_CPIM: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_CPIM);
    pub const MESSAGE_BHTTP: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_BHTTP);
    pub const MESSAGE_DELIVERY_STATUS: Encoding =
        Encoding::new(IanaEncodingMapping::MESSAGE_DELIVERY_STATUS);
    pub const MESSAGE_DISPOSITION_NOTIFICATION: Encoding =
        Encoding::new(IanaEncodingMapping::MESSAGE_DISPOSITION_NOTIFICATION);
    pub const MESSAGE_EXAMPLE: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_EXAMPLE);
    pub const MESSAGE_EXTERNAL_BODY: Encoding =
        Encoding::new(IanaEncodingMapping::MESSAGE_EXTERNAL_BODY);
    pub const MESSAGE_FEEDBACK_REPORT: Encoding =
        Encoding::new(IanaEncodingMapping::MESSAGE_FEEDBACK_REPORT);
    pub const MESSAGE_GLOBAL: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_GLOBAL);
    pub const MESSAGE_GLOBAL_DELIVERY_STATUS: Encoding =
        Encoding::new(IanaEncodingMapping::MESSAGE_GLOBAL_DELIVERY_STATUS);
    pub const MESSAGE_GLOBAL_DISPOSITION_NOTIFICATION: Encoding =
        Encoding::new(IanaEncodingMapping::MESSAGE_GLOBAL_DISPOSITION_NOTIFICATION);
    pub const MESSAGE_GLOBAL_HEADERS: Encoding =
        Encoding::new(IanaEncodingMapping::MESSAGE_GLOBAL_HEADERS);
    pub const MESSAGE_HTTP: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_HTTP);
    pub const MESSAGE_IMDN_XML: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_IMDN_XML);
    pub const MESSAGE_MLS: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_MLS);
    pub const MESSAGE_NEWS: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_NEWS);
    pub const MESSAGE_OHTTP_REQ: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_OHTTP_REQ);
    pub const MESSAGE_OHTTP_RES: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_OHTTP_RES);
    pub const MESSAGE_PARTIAL: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_PARTIAL);
    pub const MESSAGE_RFC822: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_RFC822);
    pub const MESSAGE_S_HTTP: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_S_HTTP);
    pub const MESSAGE_SIP: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_SIP);
    pub const MESSAGE_SIPFRAG: Encoding = Encoding::new(IanaEncodingMapping::MESSAGE_SIPFRAG);
    pub const MESSAGE_TRACKING_STATUS: Encoding =
        Encoding::new(IanaEncodingMapping::MESSAGE_TRACKING_STATUS);
    pub const MESSAGE_VND_SI_SIMP: Encoding =
        Encoding::new(IanaEncodingMapping::MESSAGE_VND_SI_SIMP);
    pub const MESSAGE_VND_WFA_WSC: Encoding =
        Encoding::new(IanaEncodingMapping::MESSAGE_VND_WFA_WSC);
    pub const MODEL_3MF: Encoding = Encoding::new(IanaEncodingMapping::MODEL_3MF);
    pub const MODEL_JT: Encoding = Encoding::new(IanaEncodingMapping::MODEL_JT);
    pub const MODEL_E57: Encoding = Encoding::new(IanaEncodingMapping::MODEL_E57);
    pub const MODEL_EXAMPLE: Encoding = Encoding::new(IanaEncodingMapping::MODEL_EXAMPLE);
    pub const MODEL_GLTF_JSON: Encoding = Encoding::new(IanaEncodingMapping::MODEL_GLTF_JSON);
    pub const MODEL_GLTF_BINARY: Encoding = Encoding::new(IanaEncodingMapping::MODEL_GLTF_BINARY);
    pub const MODEL_IGES: Encoding = Encoding::new(IanaEncodingMapping::MODEL_IGES);
    pub const MODEL_MESH: Encoding = Encoding::new(IanaEncodingMapping::MODEL_MESH);
    pub const MODEL_MTL: Encoding = Encoding::new(IanaEncodingMapping::MODEL_MTL);
    pub const MODEL_OBJ: Encoding = Encoding::new(IanaEncodingMapping::MODEL_OBJ);
    pub const MODEL_PRC: Encoding = Encoding::new(IanaEncodingMapping::MODEL_PRC);
    pub const MODEL_STEP: Encoding = Encoding::new(IanaEncodingMapping::MODEL_STEP);
    pub const MODEL_STEP_XML: Encoding = Encoding::new(IanaEncodingMapping::MODEL_STEP_XML);
    pub const MODEL_STEP_ZIP: Encoding = Encoding::new(IanaEncodingMapping::MODEL_STEP_ZIP);
    pub const MODEL_STEP_XML_ZIP: Encoding = Encoding::new(IanaEncodingMapping::MODEL_STEP_XML_ZIP);
    pub const MODEL_STL: Encoding = Encoding::new(IanaEncodingMapping::MODEL_STL);
    pub const MODEL_U3D: Encoding = Encoding::new(IanaEncodingMapping::MODEL_U3D);
    pub const MODEL_VND_BARY: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_BARY);
    pub const MODEL_VND_CLD: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_CLD);
    pub const MODEL_VND_COLLADA_XML: Encoding =
        Encoding::new(IanaEncodingMapping::MODEL_VND_COLLADA_XML);
    pub const MODEL_VND_DWF: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_DWF);
    pub const MODEL_VND_FLATLAND_3DML: Encoding =
        Encoding::new(IanaEncodingMapping::MODEL_VND_FLATLAND_3DML);
    pub const MODEL_VND_GDL: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_GDL);
    pub const MODEL_VND_GS_GDL: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_GS_GDL);
    pub const MODEL_VND_GTW: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_GTW);
    pub const MODEL_VND_MOML_XML: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_MOML_XML);
    pub const MODEL_VND_MTS: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_MTS);
    pub const MODEL_VND_OPENGEX: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_OPENGEX);
    pub const MODEL_VND_PARASOLID_TRANSMIT_BINARY: Encoding =
        Encoding::new(IanaEncodingMapping::MODEL_VND_PARASOLID_TRANSMIT_BINARY);
    pub const MODEL_VND_PARASOLID_TRANSMIT_TEXT: Encoding =
        Encoding::new(IanaEncodingMapping::MODEL_VND_PARASOLID_TRANSMIT_TEXT);
    pub const MODEL_VND_PYTHA_PYOX: Encoding =
        Encoding::new(IanaEncodingMapping::MODEL_VND_PYTHA_PYOX);
    pub const MODEL_VND_ROSETTE_ANNOTATED_DATA_MODEL: Encoding =
        Encoding::new(IanaEncodingMapping::MODEL_VND_ROSETTE_ANNOTATED_DATA_MODEL);
    pub const MODEL_VND_SAP_VDS: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_SAP_VDS);
    pub const MODEL_VND_USDA: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_USDA);
    pub const MODEL_VND_USDZ_ZIP: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_USDZ_ZIP);
    pub const MODEL_VND_VALVE_SOURCE_COMPILED_MAP: Encoding =
        Encoding::new(IanaEncodingMapping::MODEL_VND_VALVE_SOURCE_COMPILED_MAP);
    pub const MODEL_VND_VTU: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VND_VTU);
    pub const MODEL_VRML: Encoding = Encoding::new(IanaEncodingMapping::MODEL_VRML);
    pub const MODEL_X3D_FASTINFOSET: Encoding =
        Encoding::new(IanaEncodingMapping::MODEL_X3D_FASTINFOSET);
    pub const MODEL_X3D_XML: Encoding = Encoding::new(IanaEncodingMapping::MODEL_X3D_XML);
    pub const MODEL_X3D_VRML: Encoding = Encoding::new(IanaEncodingMapping::MODEL_X3D_VRML);
    pub const MULTIPART_ALTERNATIVE: Encoding =
        Encoding::new(IanaEncodingMapping::MULTIPART_ALTERNATIVE);
    pub const MULTIPART_APPLEDOUBLE: Encoding =
        Encoding::new(IanaEncodingMapping::MULTIPART_APPLEDOUBLE);
    pub const MULTIPART_BYTERANGES: Encoding =
        Encoding::new(IanaEncodingMapping::MULTIPART_BYTERANGES);
    pub const MULTIPART_DIGEST: Encoding = Encoding::new(IanaEncodingMapping::MULTIPART_DIGEST);
    pub const MULTIPART_ENCRYPTED: Encoding =
        Encoding::new(IanaEncodingMapping::MULTIPART_ENCRYPTED);
    pub const MULTIPART_EXAMPLE: Encoding = Encoding::new(IanaEncodingMapping::MULTIPART_EXAMPLE);
    pub const MULTIPART_FORM_DATA: Encoding =
        Encoding::new(IanaEncodingMapping::MULTIPART_FORM_DATA);
    pub const MULTIPART_HEADER_SET: Encoding =
        Encoding::new(IanaEncodingMapping::MULTIPART_HEADER_SET);
    pub const MULTIPART_MIXED: Encoding = Encoding::new(IanaEncodingMapping::MULTIPART_MIXED);
    pub const MULTIPART_MULTILINGUAL: Encoding =
        Encoding::new(IanaEncodingMapping::MULTIPART_MULTILINGUAL);
    pub const MULTIPART_PARALLEL: Encoding = Encoding::new(IanaEncodingMapping::MULTIPART_PARALLEL);
    pub const MULTIPART_RELATED: Encoding = Encoding::new(IanaEncodingMapping::MULTIPART_RELATED);
    pub const MULTIPART_REPORT: Encoding = Encoding::new(IanaEncodingMapping::MULTIPART_REPORT);
    pub const MULTIPART_SIGNED: Encoding = Encoding::new(IanaEncodingMapping::MULTIPART_SIGNED);
    pub const MULTIPART_VND_BINT_MED_PLUS: Encoding =
        Encoding::new(IanaEncodingMapping::MULTIPART_VND_BINT_MED_PLUS);
    pub const MULTIPART_VOICE_MESSAGE: Encoding =
        Encoding::new(IanaEncodingMapping::MULTIPART_VOICE_MESSAGE);
    pub const MULTIPART_X_MIXED_REPLACE: Encoding =
        Encoding::new(IanaEncodingMapping::MULTIPART_X_MIXED_REPLACE);
    pub const TEXT_1D_INTERLEAVED_PARITYFEC: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_1D_INTERLEAVED_PARITYFEC);
    pub const TEXT_RED: Encoding = Encoding::new(IanaEncodingMapping::TEXT_RED);
    pub const TEXT_SGML: Encoding = Encoding::new(IanaEncodingMapping::TEXT_SGML);
    pub const TEXT_CACHE_MANIFEST: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_CACHE_MANIFEST);
    pub const TEXT_CALENDAR: Encoding = Encoding::new(IanaEncodingMapping::TEXT_CALENDAR);
    pub const TEXT_CQL: Encoding = Encoding::new(IanaEncodingMapping::TEXT_CQL);
    pub const TEXT_CQL_EXPRESSION: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_CQL_EXPRESSION);
    pub const TEXT_CQL_IDENTIFIER: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_CQL_IDENTIFIER);
    pub const TEXT_CSS: Encoding = Encoding::new(IanaEncodingMapping::TEXT_CSS);
    pub const TEXT_CSV: Encoding = Encoding::new(IanaEncodingMapping::TEXT_CSV);
    pub const TEXT_CSV_SCHEMA: Encoding = Encoding::new(IanaEncodingMapping::TEXT_CSV_SCHEMA);
    pub const TEXT_DIRECTORY: Encoding = Encoding::new(IanaEncodingMapping::TEXT_DIRECTORY);
    pub const TEXT_DNS: Encoding = Encoding::new(IanaEncodingMapping::TEXT_DNS);
    pub const TEXT_ECMASCRIPT: Encoding = Encoding::new(IanaEncodingMapping::TEXT_ECMASCRIPT);
    pub const TEXT_ENCAPRTP: Encoding = Encoding::new(IanaEncodingMapping::TEXT_ENCAPRTP);
    pub const TEXT_ENRICHED: Encoding = Encoding::new(IanaEncodingMapping::TEXT_ENRICHED);
    pub const TEXT_EXAMPLE: Encoding = Encoding::new(IanaEncodingMapping::TEXT_EXAMPLE);
    pub const TEXT_FHIRPATH: Encoding = Encoding::new(IanaEncodingMapping::TEXT_FHIRPATH);
    pub const TEXT_FLEXFEC: Encoding = Encoding::new(IanaEncodingMapping::TEXT_FLEXFEC);
    pub const TEXT_FWDRED: Encoding = Encoding::new(IanaEncodingMapping::TEXT_FWDRED);
    pub const TEXT_GFF3: Encoding = Encoding::new(IanaEncodingMapping::TEXT_GFF3);
    pub const TEXT_GRAMMAR_REF_LIST: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_GRAMMAR_REF_LIST);
    pub const TEXT_HL7V2: Encoding = Encoding::new(IanaEncodingMapping::TEXT_HL7V2);
    pub const TEXT_HTML: Encoding = Encoding::new(IanaEncodingMapping::TEXT_HTML);
    pub const TEXT_JAVASCRIPT: Encoding = Encoding::new(IanaEncodingMapping::TEXT_JAVASCRIPT);
    pub const TEXT_JCR_CND: Encoding = Encoding::new(IanaEncodingMapping::TEXT_JCR_CND);
    pub const TEXT_MARKDOWN: Encoding = Encoding::new(IanaEncodingMapping::TEXT_MARKDOWN);
    pub const TEXT_MIZAR: Encoding = Encoding::new(IanaEncodingMapping::TEXT_MIZAR);
    pub const TEXT_N3: Encoding = Encoding::new(IanaEncodingMapping::TEXT_N3);
    pub const TEXT_PARAMETERS: Encoding = Encoding::new(IanaEncodingMapping::TEXT_PARAMETERS);
    pub const TEXT_PARITYFEC: Encoding = Encoding::new(IanaEncodingMapping::TEXT_PARITYFEC);
    pub const TEXT_PLAIN: Encoding = Encoding::new(IanaEncodingMapping::TEXT_PLAIN);
    pub const TEXT_PROVENANCE_NOTATION: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_PROVENANCE_NOTATION);
    pub const TEXT_PRS_FALLENSTEIN_RST: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_PRS_FALLENSTEIN_RST);
    pub const TEXT_PRS_LINES_TAG: Encoding = Encoding::new(IanaEncodingMapping::TEXT_PRS_LINES_TAG);
    pub const TEXT_PRS_PROP_LOGIC: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_PRS_PROP_LOGIC);
    pub const TEXT_PRS_TEXI: Encoding = Encoding::new(IanaEncodingMapping::TEXT_PRS_TEXI);
    pub const TEXT_RAPTORFEC: Encoding = Encoding::new(IanaEncodingMapping::TEXT_RAPTORFEC);
    pub const TEXT_RFC822_HEADERS: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_RFC822_HEADERS);
    pub const TEXT_RICHTEXT: Encoding = Encoding::new(IanaEncodingMapping::TEXT_RICHTEXT);
    pub const TEXT_RTF: Encoding = Encoding::new(IanaEncodingMapping::TEXT_RTF);
    pub const TEXT_RTP_ENC_AESCM128: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_RTP_ENC_AESCM128);
    pub const TEXT_RTPLOOPBACK: Encoding = Encoding::new(IanaEncodingMapping::TEXT_RTPLOOPBACK);
    pub const TEXT_RTX: Encoding = Encoding::new(IanaEncodingMapping::TEXT_RTX);
    pub const TEXT_SHACLC: Encoding = Encoding::new(IanaEncodingMapping::TEXT_SHACLC);
    pub const TEXT_SHEX: Encoding = Encoding::new(IanaEncodingMapping::TEXT_SHEX);
    pub const TEXT_SPDX: Encoding = Encoding::new(IanaEncodingMapping::TEXT_SPDX);
    pub const TEXT_STRINGS: Encoding = Encoding::new(IanaEncodingMapping::TEXT_STRINGS);
    pub const TEXT_T140: Encoding = Encoding::new(IanaEncodingMapping::TEXT_T140);
    pub const TEXT_TAB_SEPARATED_VALUES: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_TAB_SEPARATED_VALUES);
    pub const TEXT_TROFF: Encoding = Encoding::new(IanaEncodingMapping::TEXT_TROFF);
    pub const TEXT_TURTLE: Encoding = Encoding::new(IanaEncodingMapping::TEXT_TURTLE);
    pub const TEXT_ULPFEC: Encoding = Encoding::new(IanaEncodingMapping::TEXT_ULPFEC);
    pub const TEXT_URI_LIST: Encoding = Encoding::new(IanaEncodingMapping::TEXT_URI_LIST);
    pub const TEXT_VCARD: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VCARD);
    pub const TEXT_VND_DMCLIENTSCRIPT: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_DMCLIENTSCRIPT);
    pub const TEXT_VND_IPTC_NITF: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_IPTC_NITF);
    pub const TEXT_VND_IPTC_NEWSML: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_IPTC_NEWSML);
    pub const TEXT_VND_A: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_A);
    pub const TEXT_VND_ABC: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_ABC);
    pub const TEXT_VND_ASCII_ART: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_ASCII_ART);
    pub const TEXT_VND_CURL: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_CURL);
    pub const TEXT_VND_DEBIAN_COPYRIGHT: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_DEBIAN_COPYRIGHT);
    pub const TEXT_VND_DVB_SUBTITLE: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_DVB_SUBTITLE);
    pub const TEXT_VND_ESMERTEC_THEME_DESCRIPTOR: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_ESMERTEC_THEME_DESCRIPTOR);
    pub const TEXT_VND_EXCHANGEABLE: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_EXCHANGEABLE);
    pub const TEXT_VND_FAMILYSEARCH_GEDCOM: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_FAMILYSEARCH_GEDCOM);
    pub const TEXT_VND_FICLAB_FLT: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_FICLAB_FLT);
    pub const TEXT_VND_FLY: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_FLY);
    pub const TEXT_VND_FMI_FLEXSTOR: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_FMI_FLEXSTOR);
    pub const TEXT_VND_GML: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_GML);
    pub const TEXT_VND_GRAPHVIZ: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_GRAPHVIZ);
    pub const TEXT_VND_HANS: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_HANS);
    pub const TEXT_VND_HGL: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_HGL);
    pub const TEXT_VND_IN3D_3DML: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_IN3D_3DML);
    pub const TEXT_VND_IN3D_SPOT: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_IN3D_SPOT);
    pub const TEXT_VND_LATEX_Z: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_LATEX_Z);
    pub const TEXT_VND_MOTOROLA_REFLEX: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_MOTOROLA_REFLEX);
    pub const TEXT_VND_MS_MEDIAPACKAGE: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_MS_MEDIAPACKAGE);
    pub const TEXT_VND_NET2PHONE_COMMCENTER_COMMAND: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_NET2PHONE_COMMCENTER_COMMAND);
    pub const TEXT_VND_RADISYS_MSML_BASIC_LAYOUT: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_RADISYS_MSML_BASIC_LAYOUT);
    pub const TEXT_VND_SENX_WARPSCRIPT: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_SENX_WARPSCRIPT);
    pub const TEXT_VND_SI_URICATALOGUE: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_SI_URICATALOGUE);
    pub const TEXT_VND_SOSI: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_SOSI);
    pub const TEXT_VND_SUN_J2ME_APP_DESCRIPTOR: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_SUN_J2ME_APP_DESCRIPTOR);
    pub const TEXT_VND_TROLLTECH_LINGUIST: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_TROLLTECH_LINGUIST);
    pub const TEXT_VND_WAP_SI: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_WAP_SI);
    pub const TEXT_VND_WAP_SL: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_WAP_SL);
    pub const TEXT_VND_WAP_WML: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VND_WAP_WML);
    pub const TEXT_VND_WAP_WMLSCRIPT: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_VND_WAP_WMLSCRIPT);
    pub const TEXT_VTT: Encoding = Encoding::new(IanaEncodingMapping::TEXT_VTT);
    pub const TEXT_WGSL: Encoding = Encoding::new(IanaEncodingMapping::TEXT_WGSL);
    pub const TEXT_XML: Encoding = Encoding::new(IanaEncodingMapping::TEXT_XML);
    pub const TEXT_XML_EXTERNAL_PARSED_ENTITY: Encoding =
        Encoding::new(IanaEncodingMapping::TEXT_XML_EXTERNAL_PARSED_ENTITY);
    pub const VIDEO_1D_INTERLEAVED_PARITYFEC: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_1D_INTERLEAVED_PARITYFEC);
    pub const VIDEO_3GPP: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_3GPP);
    pub const VIDEO_3GPP_TT: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_3GPP_TT);
    pub const VIDEO_3GPP2: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_3GPP2);
    pub const VIDEO_AV1: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_AV1);
    pub const VIDEO_BMPEG: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_BMPEG);
    pub const VIDEO_BT656: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_BT656);
    pub const VIDEO_CELB: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_CELB);
    pub const VIDEO_DV: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_DV);
    pub const VIDEO_FFV1: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_FFV1);
    pub const VIDEO_H261: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_H261);
    pub const VIDEO_H263: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_H263);
    pub const VIDEO_H263_1998: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_H263_1998);
    pub const VIDEO_H263_2000: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_H263_2000);
    pub const VIDEO_H264: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_H264);
    pub const VIDEO_H264_RCDO: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_H264_RCDO);
    pub const VIDEO_H264_SVC: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_H264_SVC);
    pub const VIDEO_H265: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_H265);
    pub const VIDEO_H266: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_H266);
    pub const VIDEO_JPEG: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_JPEG);
    pub const VIDEO_MP1S: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_MP1S);
    pub const VIDEO_MP2P: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_MP2P);
    pub const VIDEO_MP2T: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_MP2T);
    pub const VIDEO_MP4V_ES: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_MP4V_ES);
    pub const VIDEO_MPV: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_MPV);
    pub const VIDEO_SMPTE292M: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_SMPTE292M);
    pub const VIDEO_VP8: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VP8);
    pub const VIDEO_VP9: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VP9);
    pub const VIDEO_ENCAPRTP: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_ENCAPRTP);
    pub const VIDEO_EVC: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_EVC);
    pub const VIDEO_EXAMPLE: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_EXAMPLE);
    pub const VIDEO_FLEXFEC: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_FLEXFEC);
    pub const VIDEO_ISO_SEGMENT: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_ISO_SEGMENT);
    pub const VIDEO_JPEG2000: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_JPEG2000);
    pub const VIDEO_JXSV: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_JXSV);
    pub const VIDEO_MATROSKA: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_MATROSKA);
    pub const VIDEO_MATROSKA_3D: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_MATROSKA_3D);
    pub const VIDEO_MJ2: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_MJ2);
    pub const VIDEO_MP4: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_MP4);
    pub const VIDEO_MPEG: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_MPEG);
    pub const VIDEO_MPEG4_GENERIC: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_MPEG4_GENERIC);
    pub const VIDEO_NV: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_NV);
    pub const VIDEO_OGG: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_OGG);
    pub const VIDEO_PARITYFEC: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_PARITYFEC);
    pub const VIDEO_POINTER: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_POINTER);
    pub const VIDEO_QUICKTIME: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_QUICKTIME);
    pub const VIDEO_RAPTORFEC: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_RAPTORFEC);
    pub const VIDEO_RAW: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_RAW);
    pub const VIDEO_RTP_ENC_AESCM128: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_RTP_ENC_AESCM128);
    pub const VIDEO_RTPLOOPBACK: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_RTPLOOPBACK);
    pub const VIDEO_RTX: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_RTX);
    pub const VIDEO_SCIP: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_SCIP);
    pub const VIDEO_SMPTE291: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_SMPTE291);
    pub const VIDEO_ULPFEC: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_ULPFEC);
    pub const VIDEO_VC1: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VC1);
    pub const VIDEO_VC2: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VC2);
    pub const VIDEO_VND_CCTV: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VND_CCTV);
    pub const VIDEO_VND_DECE_HD: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VND_DECE_HD);
    pub const VIDEO_VND_DECE_MOBILE: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_DECE_MOBILE);
    pub const VIDEO_VND_DECE_MP4: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VND_DECE_MP4);
    pub const VIDEO_VND_DECE_PD: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VND_DECE_PD);
    pub const VIDEO_VND_DECE_SD: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VND_DECE_SD);
    pub const VIDEO_VND_DECE_VIDEO: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_DECE_VIDEO);
    pub const VIDEO_VND_DIRECTV_MPEG: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_DIRECTV_MPEG);
    pub const VIDEO_VND_DIRECTV_MPEG_TTS: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_DIRECTV_MPEG_TTS);
    pub const VIDEO_VND_DLNA_MPEG_TTS: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_DLNA_MPEG_TTS);
    pub const VIDEO_VND_DVB_FILE: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VND_DVB_FILE);
    pub const VIDEO_VND_FVT: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VND_FVT);
    pub const VIDEO_VND_HNS_VIDEO: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_HNS_VIDEO);
    pub const VIDEO_VND_IPTVFORUM_1DPARITYFEC_1010: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_IPTVFORUM_1DPARITYFEC_1010);
    pub const VIDEO_VND_IPTVFORUM_1DPARITYFEC_2005: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_IPTVFORUM_1DPARITYFEC_2005);
    pub const VIDEO_VND_IPTVFORUM_2DPARITYFEC_1010: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_IPTVFORUM_2DPARITYFEC_1010);
    pub const VIDEO_VND_IPTVFORUM_2DPARITYFEC_2005: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_IPTVFORUM_2DPARITYFEC_2005);
    pub const VIDEO_VND_IPTVFORUM_TTSAVC: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_IPTVFORUM_TTSAVC);
    pub const VIDEO_VND_IPTVFORUM_TTSMPEG2: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_IPTVFORUM_TTSMPEG2);
    pub const VIDEO_VND_MOTOROLA_VIDEO: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_MOTOROLA_VIDEO);
    pub const VIDEO_VND_MOTOROLA_VIDEOP: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_MOTOROLA_VIDEOP);
    pub const VIDEO_VND_MPEGURL: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VND_MPEGURL);
    pub const VIDEO_VND_MS_PLAYREADY_MEDIA_PYV: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_MS_PLAYREADY_MEDIA_PYV);
    pub const VIDEO_VND_NOKIA_INTERLEAVED_MULTIMEDIA: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_NOKIA_INTERLEAVED_MULTIMEDIA);
    pub const VIDEO_VND_NOKIA_MP4VR: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_NOKIA_MP4VR);
    pub const VIDEO_VND_NOKIA_VIDEOVOIP: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_NOKIA_VIDEOVOIP);
    pub const VIDEO_VND_OBJECTVIDEO: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_OBJECTVIDEO);
    pub const VIDEO_VND_RADGAMETTOOLS_BINK: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_RADGAMETTOOLS_BINK);
    pub const VIDEO_VND_RADGAMETTOOLS_SMACKER: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_RADGAMETTOOLS_SMACKER);
    pub const VIDEO_VND_SEALED_MPEG1: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_SEALED_MPEG1);
    pub const VIDEO_VND_SEALED_MPEG4: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_SEALED_MPEG4);
    pub const VIDEO_VND_SEALED_SWF: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_SEALED_SWF);
    pub const VIDEO_VND_SEALEDMEDIA_SOFTSEAL_MOV: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_SEALEDMEDIA_SOFTSEAL_MOV);
    pub const VIDEO_VND_UVVU_MP4: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VND_UVVU_MP4);
    pub const VIDEO_VND_VIVO: Encoding = Encoding::new(IanaEncodingMapping::VIDEO_VND_VIVO);
    pub const VIDEO_VND_YOUTUBE_YT: Encoding =
        Encoding::new(IanaEncodingMapping::VIDEO_VND_YOUTUBE_YT);
} // End of impl
