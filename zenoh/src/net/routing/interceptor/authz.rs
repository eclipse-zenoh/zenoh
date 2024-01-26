use std::fmt;

// use casbin::{CoreApi, Enforcer};
use super::RoutingContext;
use serde::{Deserialize, Serialize};
use serde_json::{Result, Value};
use zenoh_config::ZenohId;
//use ZenohID;
//use zenoh_keyexpr::keyexpr_tree::box_tree::KeBoxTree;
use zenoh_protocol::network::NetworkMessage;
use zenoh_result::ZResult;

use std::{collections::HashMap, error::Error};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Action {
    Read,
    Write,
    Both,
}

pub struct NewCtx {
    pub(crate) ctx: RoutingContext<NetworkMessage>,
    pub(crate) zid: Option<ZenohId>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    sub: Subject,
    obj: String,
    action: Action,
}

pub struct RequestBuilder {
    sub: Option<Subject>,
    obj: Option<String>,
    action: Option<Action>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Subject {
    id: ZenohId,
    attributes: Option<HashMap<String, String>>, //might be mapped to u8 values eventually
}

//subject_builder (add ID, attributes, roles)

pub struct SubjectBuilder {
    id: Option<ZenohId>,
    attributes: Option<HashMap<String, String>>,
}

//request_builder (add subject, resource, action) //do we need one?

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
        //ctreas the default request
    }

    pub fn sub(&mut self, sub: impl Into<Subject>) -> &mut Self {
        //adds subject
        self.sub.insert(sub.into());
        self
    }

    pub fn obj(&mut self, obj: impl Into<String>) -> &mut Self {
        self.obj.insert(obj.into());
        self
    }

    pub fn action(&mut self, action: impl Into<Action>) -> &mut Self {
        self.action.insert(action.into());
        self
    }

    pub fn build(&mut self) -> ZResult<Request> {
        let Some(sub) = self.sub;
        let Some(obj) = self.obj;
        let Some(action) = self.action;

        Ok(Request { sub, obj, action })
    }
}

impl SubjectBuilder {
    pub fn new() -> Self {
        //creates the default request
        SubjectBuilder {
            id: None,
            attributes: None,
        }
    }

    pub fn id(&mut self, id: impl Into<ZenohId>) -> &mut Self {
        //adds subject
        self.id.insert(id.into());
        self
    }

    pub fn attributes(&mut self, attributes: impl Into<HashMap<String, String>>) -> &mut Self {
        self.attributes.insert(attributes.into());
        self
    }

    pub fn build(&mut self) -> ZResult<Subject> {
        let Some(id) = self.id;
        let attr = self.attributes;
        Ok(Subject {
            id,
            attributes: attr,
        })
    }
}

// impl fmt::Debug for Action {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         write!(f, "{:?}", self)
//     }
// }
// impl fmt::Display for Action {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         write!(f, "{:?}", self)
//     }
// }

pub trait ZAuth {
    fn authz_testing(&self, _: String, _: String, _: String) -> ZResult<bool>;
}

// impl ZAuth for Enforcer {
//     fn authz_testing(&self, zid: String, ke: String, act: String) -> ZResult<bool> {
//         /*
//         (zid, keyexpr, act): these values should be extraced from the authn code.
//         has to be atomic, to avoid another process sending the wrong info
//          */
//         if let Ok(authorized) = self.enforce((zid.clone(), ke.clone(), act.clone())) {
//             Ok(authorized)
//         } else {
//             println!("policy enforcement error");
//             Ok(false)
//         }
//     }
// }

/* replaced with PolicyEnforcer::init() function */

// pub async fn start_authz() -> Result<Enforcer> {
//     // get file value
//     let mut e = Enforcer::new("keymatch_model.conf", "keymatch_policy.csv").await?;
//     e.enable_log(true);
//     Ok(e)
// }

//struct that defines each policy (add policy type and ruleset)
#[derive(Serialize, Deserialize, Debug)]
struct Policy {
    // policy_type: u8, //for l,a,r [access-list, abac, rbac type policy] will be assuming acl for now
    sub: Subject,
    ke: String,
    action: Action,
    permission: bool,
}

#[derive(Clone)]
pub struct PolicyEnforcer {
    policy_config: HashMap<String, HashMap<String, String>>,
}

impl PolicyEnforcer {
    pub fn init() -> ZResult<Self> {
        /*
           Initializs the policy for the control logic
           loads policy into memory from file/network path
           creates the policy hashmap with the ke-tries for ke matching
           should have polic-type in the mix here...need to verify
        */
        //policy should be derived from config/file (hardcoding it for now)
        //config for local static policy
        let static_policy = r#"{
                            ["subject":{"id": "muyid", "attributes": "location"},"ke":"my_ke","action":"Read","permission":true],
                            ["subject":{"id": "muyid", "attributes": "location"},"ke":"myke","action":"Read","permission":true],
                            ["subject":{"id": "muyid", "attributes": "location"},"ke":"myke","action":"Read","permission":true],
                            ["subject":{"id": "muyid", "attributes": "location"},"ke":"myke","action":"Read","permission":true]
                        }"#;
        //desearlize to policy
        let get_policy: Policy = serde_json::from_str(static_policy)?;
        println!("print policy {:?}", get_policy);
        let policy = Self::build_policy_map(get_policy);

        let pe = Self {
            policy_config: policy,
        };

        //also should start the logger here
        Ok(pe)
    }

    pub fn build_policy_map(policy: Policy) {

        //convert policy to vector of hashmap (WIP)
        /*
                       policy = subject : [ rule_1,
                                           rule_2,
                                           ...
                                           rule_n
                                        ]
                       where rule_i = action_i : (ke_tree_deny, ke_tree_allow) that deny/allow action_i
        */
    }
    pub fn policy_enforcement_point(&self, new_ctx: NewCtx, action: Action) -> ZResult<bool> {
        /*
           input: msg body
           output: allow/deny
           function: depending on the msg, builds the subject, builds the request, passes the request to policy_decision_point()
                    collects result from PDP and then uses that allow/deny output to block or pass the msg to routing table
        */

        let Some(ke) = new_ctx.ctx.full_expr();
        let zid = new_ctx.zid.unwrap();
        //build subject here

        let subject = SubjectBuilder::new().id(zid).build()?; //.attributes(None).build();
        let request = RequestBuilder::new()
            .sub(subject)
            .obj(ke)
            .action(action)
            .build()?;
        let decision = self.policy_decision_point(request)?;
        Ok(false)
    }
    pub fn permission_request_builder(
        msg: zenoh_protocol::network::NetworkMessage,
        action: Action,
    ) {

        /*
           input: msg body
           output: (sub,ke,act)
           function: extract relevant info from the incoming msg body
                        build the subject [ID, Attributes and Roles]
                        then use that to build the request [subject, key-expression, action ]
                        return request to PEP
        */
        /*
           PHASE1: just extract the ID (zid?) from the msg; can later add attributes to the list. have a struct with ID and attributes field (both Option)
        */
    }
    pub fn policy_decision_point(&self, request: Request) -> ZResult<bool> {
        /*
            input: (request)
            output: true(allow)/false(deny)
            function: process the request from PEP against policy list
                    policy list will(might) be a hashmap of subject:rules_vector (test and discuss)
        */
        /*
           PHASE1: policy decisions are hardcoded against the policy list; can later update them using a config file.
        */
        //representaiton of policy list as a hashmap of trees?
        // HashMap<id,Hashmap<action,KeBoxTree>>
        /* use KeTrees for mapping R/W values? //need to check this out
            tried KeTrees, didn't work
            need own algorithm for pattern matching via modified trie-search
        */
        Ok(false)
    }

    pub fn policy_resource_point() {

        /*
           input: config file value along with &self
           output: loads the appropriate policy into the memory and returns back self (now with added policy info); might also select AC type (ACL or ABAC)
        */

        /*
           PHASE1: just have a vector of structs containing these values; later we can load them here from config
        */
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn testing_acl_rules() {
        //sample test stub
        let result = 1 + 1;
        assert_eq!(result, 2);
    }
    #[test]
    fn testing_abac_rules() {
        //sample test stub
        let result = 1 + 1;
        assert_eq!(result, 2);
    }
}
