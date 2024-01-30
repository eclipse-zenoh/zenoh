use std::fs::File;
use std::io::Read;
use std::{fmt, hash::Hash};

use super::RoutingContext;
use csv::ReaderBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{Result, Value};
use zenoh_config::ZenohId;

use zenoh_protocol::network::NetworkMessage;
use zenoh_result::ZResult;

use fr_trie::glob::acl::{Acl, AclTrie, Permissions};
use fr_trie::glob::GlobMatcher;

use std::{collections::HashMap, error::Error};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum Action {
    None,
    Read,
    Write,
    Both,
}

pub struct NewCtx<'a> {
    pub(crate) ke: &'a str,
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

type KeTree = AclTrie;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Subject {
    id: ZenohId,
    attributes: Option<Vec<String>>, //might be mapped to other types eventually
}

//subject_builder (add ID, attributes, roles)

pub struct SubjectBuilder {
    id: Option<ZenohId>,
    attributes: Option<Vec<String>>,
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
        let sub = self.sub.clone().unwrap();
        let obj = self.obj.clone().unwrap();
        let action = self.action.clone().unwrap();

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

    pub fn attributes(&mut self, attributes: impl Into<Vec<String>>) -> &mut Self {
        self.attributes.insert(attributes.into());
        self
    }

    pub fn build(&mut self) -> ZResult<Subject> {
        let id = self.id.unwrap();
        let attr = self.attributes.clone();
        Ok(Subject {
            id,
            attributes: attr,
        })
    }
}

// pub trait ZAuth {
//     fn authz_testing(&self, _: String, _: String, _: String) -> ZResult<bool>;
// }

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
#[derive(Serialize, Deserialize, Clone)]
pub struct Rule {
    // policy_type: u8, //for l,a,r [access-list, abac, rbac type policy] will be assuming acl for now
    sub: Subject,
    ke: String,
    action: Action,
    permission: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubAct(Subject, Action);

#[derive(Clone)] //, PartialEq, Eq, Hash)]
pub struct PolicyEnforcer(HashMap<SubAct, KeTree>); //need to add tries here

#[derive(Clone, PartialEq, Eq, Hash)]

pub struct KeTrie {}

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
        // let policy_info = Self::policy_resource_point().unwrap();

        //desearlize to vector of rules
        // let rule_set: Vec<Rule> = serde_json::from_str(policy_info)?;

        let rule_set = Self::policy_resource_point().unwrap();
        let pe = Self::build_policy_map(rule_set).expect("policy not established");
        //also should start the logger here
        Ok(pe)
    }

    pub fn build_policy_map(rule_set: Vec<Rule>) -> ZResult<PolicyEnforcer> {
        //let pe: PolicyEnforcer;
        let mut policy = PolicyEnforcer(HashMap::new());

        //convert vector of rules to a hashmap mapping subact to ketree (WIP)
        /*
                       policy = subject : [ rule_1,
                                           rule_2,
                                           ...
                                           rule_n
                                        ]
                       where rule_i = action_i : (ke_tree_deny, ke_tree_allow) that deny/allow action_i
        */

        // let mut policy = Policy(HashMap::new());
        //now create a hashmap for ketrees ((sub->action)->ketree)
        //  let rules: HashMap<String, KeTree>; //u8 is 0 for disallowed and 1 for allowed??
        //iterate through the map to get
        for v in rule_set {
            //for each rule
            //extract subject and action

            /*
               for now permission not allowed means it will not be added to the allow trie
            */
            let perm = v.permission;
            if !perm {
                continue;
            }
            let sub = v.sub;
            let action = v.action;
            let ke = v.ke;
            //let perm = v.permission;
            //create subact
            let subact = SubAct(sub, action);
            //match subact in the policy hashmap
            #[allow(clippy::map_entry)]
            if !policy.0.contains_key(&subact) {
                //create new entry for subact + ketree
                let mut ketree = KeTree::new();
                ketree.insert(Acl::new(&ke), Permissions::READ);
                // ketree.insert(ke,1).unwrap();    //1 for allowed??
                policy.0.insert(subact, ketree);
            } else {
                let ketree = policy.0.get_mut(&subact).unwrap();
                // ketree.insert(ke,1).unwrap();    //1 for allowed??
                // let mut ketree = KeTree::new();
                // let x = Permissions::READ;
                ketree.insert(Acl::new(&ke), Permissions::READ);
                // policy.0.insert(subact,ketree);
            }
        }
        //return policy;

        Ok(policy)
    }
    pub fn policy_enforcement_point(&self, new_ctx: NewCtx, action: Action) -> ZResult<bool> {
        /*
           input: msg body
           output: allow/deny
           function: depending on the msg, builds the subject, builds the request, passes the request to policy_decision_point()
                    collects result from PDP and then uses that allow/deny output to block or pass the msg to routing table
        */

        let ke = new_ctx.ke;
        let zid = new_ctx.zid.unwrap();
        //build subject here

        let subject = SubjectBuilder::new().id(zid).build()?;
        let request = RequestBuilder::new()
            .sub(subject)
            .obj(ke)
            .action(action)
            .build()?;
        let decision = self.policy_decision_point(request)?;
        Ok(decision)
    }
    // pub fn permission_request_builder(
    //     msg: zenoh_protocol::network::NetworkMessage,
    //     action: Action,
    // ) {

    //     /*
    //        input: msg body
    //        output: (sub,ke,act)
    //        function: extract relevant info from the incoming msg body
    //                     build the subject [ID, Attributes and Roles]
    //                     then use that to build the request [subject, key-expression, action ]
    //                     return request to PEP
    //     */
    //     /*
    //        PHASE1: just extract the ID (zid?) from the msg; can later add attributes to the list. have a struct with ID and attributes field (both Option)
    //     */
    // }

    pub fn policy_decision_point(&self, request: Request) -> ZResult<bool> {
        /*
            input: (request)
            output: true(allow)/false(deny)
            function: process the request from PEP against the policy (self)
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

        //extract subject and action from request and create subact [this is our key for hashmap]
        let subact = SubAct(request.sub, request.action);
        let ke = request.obj;
        // type policymap =
        match self.0.get(&subact) {
            Some(ktrie) => {
                // check if request ke has a match in ke-trie
                // if ke in ke-trie, then Ok(true) else Ok(false)
                //let trie = self.0.get.(&subact).clone();
                let result = ktrie.get_merge::<GlobMatcher>(&Acl::new(&ke));
                if let Some(value) = result {
                    return Ok(true);
                }
            }
            None => return Ok(false),
        }

        Ok(false)
    }

    pub fn policy_resource_point() -> ZResult<Vec<Rule>> {
        /*
           input: config file value along with &self
           output: loads the appropriate policy into the memory and returns back self (now with added policy info); might also select AC type (ACL or ABAC)
        */

        /*
           PHASE1: just have a vector of structs containing these values; later we can load them here from config
        */
        #[derive(Serialize, Deserialize, Clone)]

        struct Rules(Vec<Rule>); // = Vec::new();
                                 // let mut rdr = ReaderBuilder::new()
                                 //     .has_headers(true)
                                 //     .from_path("rules.csv")
                                 //     .unwrap();
        let mut file = File::open("rules.json5").unwrap();
        let mut buff = String::new();
        file.read_to_string(&mut buff).unwrap();

        let rulevec: Rules = serde_json::from_str(&buff).unwrap();
        // for result in rdr.deserialize() {
        //     if let Ok(rec) = result {
        //         let record: Rule = rec;
        //         rule_set.push(record);
        //     } else {
        //         bail!("unable to parse json file");
        //     }
        // }

        Ok(rulevec.0)
    }
}

// fn ketrie_matcher(ke,ketrie){

// }

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
