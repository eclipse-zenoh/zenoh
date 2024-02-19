use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::hash::Hash;
use zenoh_config::AclConfig;
use zenoh_keyexpr::keyexpr;
use zenoh_keyexpr::keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, KeBoxTree};
use zenoh_result::ZResult;
#[derive(Clone, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum Action {
    None,
    Read,
    Write,
    DeclareSub,
    Delete,
    DeclareQuery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub(crate) sub: Attribute, //removed String
    pub(crate) obj: String,
    pub(crate) action: Action,
}

pub struct RequestBuilder {
    sub: Option<Attribute>, //removed Attribute
    obj: Option<String>,
    action: Option<Action>,
}

type KeTreeRule = KeBoxTree<bool>;

impl RequestBuilder {
    pub fn default() -> Self {
        RequestBuilder {
            sub: None,
            obj: None,
            action: None,
        }
    }
    pub fn new() -> Self {
        RequestBuilder::default()
    }

    pub fn sub(&mut self, sub: impl Into<Attribute>) -> &mut Self {
        //adds subject
        let _ = self.sub.insert(sub.into());
        self
    }

    pub fn obj(&mut self, obj: impl Into<String>) -> &mut Self {
        let _ = self.obj.insert(obj.into());
        self
    }

    pub fn action(&mut self, action: impl Into<Action>) -> &mut Self {
        let _ = self.action.insert(action.into());
        self
    }

    pub fn build(&mut self) -> ZResult<Request> {
        let sub = self.sub.clone().unwrap();
        let obj = self.obj.clone().unwrap();
        let action = self.action.clone().unwrap();

        Ok(Request { sub, obj, action })
    }
}

type SubActPolicy = FxHashMap<SubAct, KeTreeRule>; //replaces SinglePolic

pub struct PolicyEnforcer {
    acl_enabled: bool,
    default_deny: bool,
    attribute_list: Option<Vec<String>>, //should have all attribute names
    policy_list: Option<Vec<SubActPolicy>>, //stores policy-map for ID and each attribute
}

pub struct PolicyInformation {
    policy_definition: String,
    attribute_list: Vec<String>, //list of attribute names in string
    policy_rules: Vec<AttributeRules>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AttributeRules {
    attribute_name: String,
    attribute_rules: Vec<AttributeRule>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AttributeRule {
    sub: Attribute, //changed from string
    ke: String,
    action: Action,
    permission: bool,
}
use zenoh_config::ZenohId;

#[derive(Serialize, Debug, Deserialize, Eq, PartialEq, Hash, Clone)]
//#[serde(tag = "sub")]
#[serde(untagged)]
pub enum Attribute {
    UserID(ZenohId),
    NetworkType(String),  //clarify
    MetadataType(String), //clarify
}
#[derive(Serialize, Debug, Deserialize, Eq, PartialEq, Hash, Clone)]
pub struct SubAct(Attribute, Action); //changed from String to Attribute

#[derive(Debug)]
pub struct RequestInfo {
    pub sub: Vec<Attribute>,
    pub ke: String,
    pub action: Action,
}

impl PolicyEnforcer {
    pub fn new() -> ZResult<PolicyEnforcer> {
        Ok(PolicyEnforcer {
            acl_enabled: true,
            default_deny: true,
            attribute_list: None,
            policy_list: None,
        })
    }
    pub fn init(&mut self, acl_config: AclConfig) -> ZResult<()> {
        /*
           Initializes the policy for the control logic
           loads policy into memory from file/network path
           creates the policy hashmap with the ke-tries for ke matching
           can have policy-type in the mix here...need to verify
        */
        self.acl_enabled = acl_config.enabled.unwrap();
        self.default_deny = acl_config.default_deny.unwrap();
        let file_path = acl_config.policy_file.unwrap();
        let policy_information = self.policy_resource_point(&file_path)?;
        self.attribute_list = Some(policy_information.attribute_list);
        let _policy_definition = policy_information.policy_definition;

        //create policy_list for sub|act:ke from the info we have

        self.build_policy_map(
            self.attribute_list.clone().unwrap(),
            policy_information.policy_rules,
        )
        .expect("policy not established");
        //logger should start here
        Ok(())
    }
    pub fn build_policy_map(
        &mut self,
        attribute_list: Vec<String>,
        policy_rules_vector: Vec<AttributeRules>,
    ) -> ZResult<()> {
        /*
            representaiton of policy list as a vector of hashmap of trees
            each hashmap maps a subject (ID/atttribute) to a trie of allowed values
        */
        //for each attrribute in the list, get rules, create map and push into rules_vector
        let mut pm: Vec<SubActPolicy> = Vec::new();
        for (i, _) in attribute_list.iter().enumerate() {
            let rm = self.get_rules_list(policy_rules_vector[i].attribute_rules.clone())?;
            pm.push(rm);
        }
        self.policy_list = Some(pm);

        Ok(())
    }
    pub fn get_rules_list(&self, rule_set: Vec<AttributeRule>) -> ZResult<SubActPolicy> {
        let mut policy: SubActPolicy = FxHashMap::default();
        for v in rule_set {
            //  for now permission being false means this KE will not be inserted into the trie of allowed KEs
            let perm = v.permission;
            if !perm {
                continue;
            }
            let sub = v.sub;
            let ke = v.ke;
            let subact = SubAct(sub, v.action);
            //match subject to the policy hashmap
            #[allow(clippy::map_entry)]
            if !policy.contains_key(&subact) {
                //create new entry for subject + ke-tree
                let mut ketree = KeTreeRule::new();
                ketree.insert(keyexpr::new(&ke)?, true);
                policy.insert(subact, ketree);
            } else {
                let ketree = policy.get_mut(&subact).unwrap();
                ketree.insert(keyexpr::new(&ke)?, true);
            }
        }
        Ok(policy)
    }

    pub fn policy_resource_point(&self, file_path: &str) -> ZResult<PolicyInformation> {
        //read file
        #[derive(Deserialize)]
        struct GetPolicyFile {
            policy_definition: String,
            rules: Vec<AttributeRules>,
        }

        let policy_file_info: GetPolicyFile = {
            let data = fs::read_to_string(file_path).expect("error reading file");
            serde_json::from_str(&data).expect("error parsing from json to struct")
        };

        //get the rules mentioned in the policy definition
        let enforced_attributes = policy_file_info
            .policy_definition
            .split(' ')
            .collect::<Vec<&str>>();

        let complete_ruleset = policy_file_info.rules;
        let mut attribute_list: Vec<String> = Vec::new();
        let mut policy_rules: Vec<AttributeRules> = Vec::new();
        for rule in complete_ruleset.iter() {
            if enforced_attributes.contains(&rule.attribute_name.as_str()) {
                attribute_list.push(rule.attribute_name.clone());
                policy_rules.push(rule.clone())
            }
        }

        let policy_definition = policy_file_info.policy_definition;

        Ok(PolicyInformation {
            policy_definition,
            attribute_list,
            policy_rules,
        })
    }

    pub fn policy_enforcement_point(&self, request_info: RequestInfo) -> ZResult<bool> {
        /*
           input: new_context and action (sub,act for now but will need attribute values later)
           output: allow/denyca q
           function: depending on the msg, builds the subject, builds the request, passes the request to policy_decision_point()
                    collects result from PDP and then uses that allow/deny output to block or pass the msg to routing table
        */

        /*
        for example, if policy_defintiion = "userid and nettype"
        our request will be 2 different calls to pdp with different rewuest values
        so we will get val1= matcher_function(sub=userid...), val2=matcher_function(sub=nettype...)
        and our policy function will be val1 and val2 (from "userid and nettype" given in the policy_defintiion)
        and matcher function be mathcer_function(request)
        in our pdp, we will call matcher("subval")
         */
        //return Ok(true);
        //  println!("request info: {:?}", request_info);
        let obj = request_info.ke;
        let mut decisions: Vec<bool> = Vec::new(); //to store all decisions for each subject in list
                                                   // let subject_list = request_info.sub.0;

        //    loop through the attributes and store decision for each
        for (attribute_index, val) in request_info.sub.into_iter().enumerate() {
            // val.0 is attribute name, val.1 is attribute value
            //build request
            let request = RequestBuilder::new()
                .sub(val)
                .obj(obj.clone())
                .action(request_info.action.clone())
                .build()?;

            decisions.push(self.policy_decision_point(attribute_index, request));
        }
        let decision: bool = decisions[0]; //should run a function over the decisons vector
        Ok(decision)
    }

    pub fn policy_decision_point(&self, index: usize, request: Request) -> bool {
        /*
            input: (request)
            output: true(allow)/false(deny)
            function: process the request received from PEP against the policy (self)
                    the policy list is chosen based on the policy-type specified in the rules file
                    policy list is be a hashmap of subject->ketries (test and discuss)
        */

        //compare the request to the vec of values...matching depends on the value of the policy type

        //return true;
        let ke = request.obj;
        let sub = request.sub;
        let action = request.action;
        let subact = SubAct(sub, action);
        //find index of attribute name in attribute list
        //then use attribute_rules from same index
        if let Some(policy_list) = &self.policy_list {
            match policy_list[index].get(&subact) {
                Some(ktrie) => {
                    let result = ktrie.nodes_including(keyexpr::new(&ke).unwrap()).count();
                    return result != 0;
                }
                None => return false,
            }
        }
        false
    }

    pub fn get_attribute_list(&self) -> Option<Vec<String>> {
        self.attribute_list.clone()
    }
}
