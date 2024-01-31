use std::fs::File;
use std::hash::Hash;
use std::io::Read;

//use super::RoutingContext;
use fr_trie::key::ValueMerge;
use serde::{Deserialize, Serialize};
use zenoh_config::ZenohId;
//use zenoh_protocol::network::NetworkMessage;

use zenoh_result::ZResult;

use fr_trie::glob::acl::Acl;
use fr_trie::glob::GlobMatcher;
use fr_trie::trie::Trie;

use std::collections::HashMap;

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
#[derive(Clone, Debug)]

pub enum Permissions {
    Deny,
    Allow,
}

impl ValueMerge for Permissions {
    fn merge(&self, _other: &Self) -> Self {
        self.clone()
    }

    fn merge_mut(&mut self, _other: &Self) {}
}

type KeTree = Trie<Acl, Permissions>;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
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

//struct that defines a single rule in the access-control policy
#[derive(Serialize, Deserialize, Clone)]
pub struct Rule {
    sub: Subject,
    ke: String,
    action: Action,
    permission: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SubAct(Subject, Action);

#[derive(Clone)]
pub struct PolicyEnforcer(HashMap<SubAct, KeTree>);
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
        let rule_set = Self::policy_resource_point("rules_test_thr.json5").unwrap();
        let pe = Self::build_policy_map(rule_set).expect("policy not established");
        //also should start the logger here
        Ok(pe)
    }

    pub fn build_policy_map(rule_set: Vec<Rule>) -> ZResult<PolicyEnforcer> {
        //convert vector of rules to a hashmap mapping subact to ketrie
        /*
            representaiton of policy list as a hashmap of trees
            tried KeTrees, didn't work
            using fr-trie for now as a placeholder for key-matching
        */

        let mut policy = PolicyEnforcer(HashMap::new());
        //create a hashmap for ketries ((sub,action)->ketrie) from the vector of rules
        for v in rule_set {
            //  for now permission being false means this ke will not be inserted into the trie of allowed ke's
            let perm = v.permission;
            if !perm {
                continue;
            }
            let sub = v.sub;
            let action = v.action;
            let ke = v.ke;

            //create subact
            let subact = SubAct(sub, action);
            //match subact in the policy hashmap
            #[allow(clippy::map_entry)]
            if !policy.0.contains_key(&subact) {
                //create new entry for subact + ketree
                let mut ketree = KeTree::new();
                //ketree.insert(Acl::new(&ke), Permissionssions::READ);
                ketree.insert(Acl::new(&ke), Permissions::Allow);
                policy.0.insert(subact, ketree);
            } else {
                let ketree = policy.0.get_mut(&subact).unwrap();
                //ketree.insert(Acl::new(&ke), Permissionssions::READ);
                ketree.insert(Acl::new(&ke), Permissions::Allow);
            }
        }
        Ok(policy)
    }
    pub fn policy_enforcement_point(&self, new_ctx: NewCtx, action: Action) -> ZResult<bool> {
        /*
           input: new_context and action (sub,act for now but will need attribute values later)
           output: allow/deny
           function: depending on the msg, builds the subject, builds the request, passes the request to policy_decision_point()
                    collects result from PDP and then uses that allow/deny output to block or pass the msg to routing table
        */

        //get keyexpression and zid for the request; attributes will be added at this point (phase 2)
        let ke = new_ctx.ke;
        let zid = new_ctx.zid.unwrap();
        //build subject
        let subject = SubjectBuilder::new().id(zid).build()?;
        //build request
        let request = RequestBuilder::new()
            .sub(subject)
            .obj(ke)
            .action(action)
            .build()?;

        //call PDP
        let decision = self.policy_decision_point(request)?;
        Ok(decision)
    }
    pub fn policy_decision_point(&self, request: Request) -> ZResult<bool> {
        /*
            input: (request)
            output: true(allow)/false(deny)
            function: process the request received from PEP against the policy (self)
                    policy list is be a hashmap of (subject,action)->ketries (test and discuss)
        */

        //get subject and action from request and create subact [this will be our key for hashmap]
        let subact = SubAct(request.sub, request.action);
        let ke = request.obj;
        match self.0.get(&subact) {
            Some(ktrie) => {
                // check if request ke has a match in ke-trie; if ke in ketrie, then Ok(true) else Ok(false)
                let result = ktrie.get_merge::<GlobMatcher>(&Acl::new(&ke));
                if let Some(_value) = result {
                    return Ok(true);
                }
            }
            None => return Ok(false),
        }

        Ok(false)
    }

    pub fn policy_resource_point(file_path: &str) -> ZResult<Vec<Rule>> {
        /*
           input: path to rules.json file
           output: loads the appropriate policy into the memory and returns back a vector of rules;
           * might also be the point to select AC type (ACL, ABAC etc)?? *
        */
        #[derive(Serialize, Deserialize, Clone)]
        struct Rules(Vec<Rule>);

        let mut file = File::open(file_path).unwrap();
        let mut buff = String::new();
        file.read_to_string(&mut buff).unwrap();
        let rulevec: Rules = serde_json::from_str(&buff).unwrap();
        Ok(rulevec.0)
    }
}

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
