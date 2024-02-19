use std::collections::HashMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::net::Ipv4Addr;
use std::str::FromStr;

use fr_trie::glob::GlobMatcher;

use serde::{Deserialize, Serialize};
use zenoh_config::ZenohId;

use fr_trie::glob::acl::Acl;
use rustc_hash::FxHashMap;
use zenoh_result::ZResult;

use fr_trie::trie::Trie;

use bitflags::bitflags;

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct ActionFlag(u8);

bitflags! {
    impl ActionFlag: u8 {
        const None = 0b00000000;
        const Read = 0b00000001;
        const Write = 0b00000010;
        const DeclareSub = 0b00000100;
        const Delete = 0b00001000;
        const DeclareQuery = 0b00010000;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum Action {
    None,
    Read,
    Write,
    DeclareSub,
    Delete,
    DeclareQuery,
}

pub struct NewCtx<'a> {
    pub(crate) ke: &'a str,
    pub(crate) zid: ZenohId,
    pub(crate) attributes: Option<Attributes>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub(crate) sub: Subject,
    pub(crate) obj: String,
    pub(crate) action: ActionFlag,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRequest {
    pub(crate) sub: PolicySubject,
    pub(crate) obj: String,
    pub(crate) action: ActionFlag,
}

pub struct RequestBuilder {
    sub: Option<Subject>,
    obj: Option<String>,
    action: Option<ActionFlag>,
}
//#[derive(Clone, Debug)]

// pub enum Permissions {
//     Deny,
//     Allow,
// }

type KeTreeFast = Trie<Acl, ActionFlag>;

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]

pub struct Attributes(HashMap<String, String>);

// impl Into<Attributes> for Option<Attributes> {
//     fn into(self) -> Attributes {
//         self::Attributes
//     }
// }

impl std::hash::Hash for Attributes {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut pairs: Vec<_> = self.0.iter().collect();
        pairs.sort_by_key(|i| i.0);
        Hash::hash(&pairs, state);
    }

    fn hash_slice<H: std::hash::Hasher>(data: &[Self], state: &mut H)
    where
        Self: Sized,
    {
        for piece in data {
            piece.hash(state)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct Subject {
    pub(crate) id: ZenohId,
    pub(crate) attributes: Option<Attributes>, //might be mapped to other types eventually
}

//subject_builder (add ID, attributes, roles)

pub struct SubjectBuilder {
    id: Option<ZenohId>,
    attributes: Option<Attributes>,
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
        let _ = self.sub.insert(sub.into());
        self
    }

    pub fn obj(&mut self, obj: impl Into<String>) -> &mut Self {
        let _ = self.obj.insert(obj.into());
        self
    }

    pub fn action(&mut self, action: impl Into<ActionFlag>) -> &mut Self {
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

impl SubjectBuilder {
    pub fn new() -> Self {
        //creates a new request
        SubjectBuilder {
            id: None,
            attributes: None,
        }
    }

    pub fn id(&mut self, id: impl Into<ZenohId>) -> &mut Self {
        //adds subject
        let _ = self.id.insert(id.into());
        self
    }

    pub fn attributes(&mut self, attributes: impl Into<Attributes>) -> &mut Self {
        let _ = self.attributes.insert(attributes.into());
        self
    }

    pub fn build(&mut self) -> ZResult<Subject> {
        let id = self.id.unwrap();
        let attr = self.attributes.as_ref();
        Ok(Subject {
            id,
            attributes: attr.cloned(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub enum PolicySubject {
    Id(ZenohId),
    Attribute(Attribute),
}

//single attribute per policy check
#[derive(Eq, Clone, Hash, PartialEq, Serialize, Debug, Deserialize)]
pub enum Attribute {
    IPRange(Ipv4Addr),
    Networktype(u8), //1 for wifi,2 for lan etc
}
//#[derive(Eq, Hash, PartialEq)]
type SinglePolicy = FxHashMap<PolicySubject, KeTreeFast>;
pub struct NewPolicyEnforcer(
    u8,                    //stores types of policies (for now just 1,2,3 for userid,attribute,both)
    pub Vec<SinglePolicy>, //stores
);

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct KeTrie {}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PolicyRule {
    sub: PolicySubject,
    ke: String,
    action: Action,
    permission: bool,
}
impl NewPolicyEnforcer {
    pub fn new() -> ZResult<NewPolicyEnforcer> {
        // PolicyEnforcer
        Ok(NewPolicyEnforcer(0, Vec::new()))
    }

    pub fn init(&mut self) -> ZResult<()> {
        /*
           Initializes the policy for the control logic
           loads policy into memory from file/network path
           creates the policy hashmap with the ke-tries for ke matching
           can have policy-type in the mix here...need to verify
        */
        //set the policy type to 1,2,3 depending on the user input in the "rules" file

        let (rule_type, rule_set) = self.policy_resource_point("rules_test_thr.json5").unwrap();
        self.build_policy_map(rule_type, rule_set)
            .expect("policy not established");

        /* setup a temporary variable here to hold all the values */
        //also should start the logger here
        Ok(())
    }

    pub fn build_policy_map(&mut self, rule_type: u8, rule_set: Vec<PolicyRule>) -> ZResult<()> {
        //convert vector of rules to a hashmap mapping subact to ketrie
        /*
            representaiton of policy list as a vector of hashmap of trees
            each hashmap maps a subject (ID/atttribute) to a trie of allowed values
            using fr-trie for now as a placeholder for key-matching
        */
        self.0 = rule_type;
        let map_subject_policy = &mut self.1;
        match rule_type {
            1 => map_subject_policy.push(Self::build_id_map(rule_set)),

            2 => map_subject_policy.push(Self::build_attribute_map(rule_set)),
            3 | 4 => {
                map_subject_policy.push(Self::build_id_map(rule_set.clone()));
                map_subject_policy.push(Self::build_attribute_map(rule_set));
            }
            _ => bail!("bad entry for type"),
        }
        Ok(())
    }

    pub fn build_id_map(rule_set: Vec<PolicyRule>) -> SinglePolicy {
        let mut policy: SinglePolicy = FxHashMap::default();
        for v in rule_set {
            //  for now permission being false means this ke will not be inserted into the trie of allowed ke's
            let perm = v.permission;
            if !perm {
                continue;
            }
            let sub = v.sub;
            let ke = v.ke;
            let action_flag = match v.action {
                Action::Read => ActionFlag::Read,
                Action::Write => ActionFlag::Write,
                Action::None => ActionFlag::None,
                Action::DeclareSub => ActionFlag::DeclareSub,
                Action::Delete => ActionFlag::Delete,
                Action::DeclareQuery => ActionFlag::DeclareQuery,
            };

            //match subject to the policy hashmap
            #[allow(clippy::map_entry)]
            if !policy.contains_key(&sub) {
                //create new entry for subject + ke-tree
                let mut ketree = KeTreeFast::new();
                ketree.insert(Acl::new(&ke), action_flag);
                policy.insert(sub, ketree);
            } else {
                let ketree = policy.get_mut(&sub).unwrap();
                let old_flag = ketree.get::<GlobMatcher>(&Acl::new(&ke)).unwrap();
                ketree.insert(Acl::new(&ke), action_flag | old_flag); //update old flag
            }
        }
        policy
    }

    fn build_attribute_map(rule_set: Vec<PolicyRule>) -> SinglePolicy {
        let x: SinglePolicy = FxHashMap::default();
        return x;
    }

    pub fn policy_enforcement_point(&self, new_ctx: NewCtx, action: ActionFlag) -> ZResult<bool> {
        /*
           input: new_context and action (sub,act for now but will need attribute values later)
           output: allow/denyca
           function: depending on the msg, builds the subject, builds the request, passes the request to policy_decision_point()
                    collects result from PDP and then uses that allow/deny output to block or pass the msg to routing table
        */

        //get keyexpression and zid for the request; attributes will be added at this point (phase 2)

        let ke = new_ctx.ke;
        let zid = new_ctx.zid;
        let attribute = new_ctx.attributes;
        // if let Some(value) = new_ctx.attributes {
        //     attribute = value;
        // }
        // let subject = SubjectBuilder::new().id(zid).build()?;
        //build request
        // let request = RequestBuilder::new()
        // .sub(subject)
        // .obj(ke)
        // .action(action)
        // .build()?;

        let subject = PolicySubject::Id(zid);
        let request = NewRequest {
            sub: subject,
            obj: ke.to_owned(),
            action,
        };
        let decision = self.policy_decision_point(request);
        Ok(decision)
    }

    pub fn policy_decision_point(&self, request: NewRequest) -> bool {
        /*
            input: (request)
            output: true(allow)/false(deny)
            function: process the request received from PEP against the policy (self)
                    the policy list is chosen based on the policy-type specified in the rules file
                    policy list is be a hashmap of subject->ketries (test and discuss)
        */

        //compare the request to the vec of values...matching depends on the value of the policy type
        match self.0 {
            1 => {
                //check the id map (value 0)
                return self.matching_algo(0, request);
            }
            2 => {
                //check the attribute map (value 1)
                return self.matching_algo(1, request);
            }
            3 => {
                //check both the maps and do an OR (either ID or attribute should match for allow)
                return self.matching_algo(0, request.clone()) || self.matching_algo(1, request);
            }
            4 => {
                //check both the maps and do AND (both ID and attribute should match for allow)
                return self.matching_algo(0, request.clone()) && self.matching_algo(1, request);
            }
            _ => {
                //wrong value; deny request
                false
            }
        };

        false
    }

    pub fn matching_algo(&self, matching_type: usize, request: NewRequest) -> bool {
        //  return true;
        let ke = request.obj;
        let sub = request.sub;
        let action = request.action;

        match self.1[matching_type].get(&sub) {
            Some(ktrie) => {
                // check if request ke has a match in ke-trie; if ke in ketrie, then true (allow) else false (deny)
                let result = ktrie.get::<GlobMatcher>(&Acl::new(&ke));
                match result {
                    Some(value) => (value & action) != ActionFlag::None,
                    None => false,
                }
            }
            None => false,
        }
    }

    pub fn policy_resource_point(&self, file_path: &str) -> ZResult<(u8, Vec<PolicyRule>)> {
        /*
           input: path to rules.json file
           output: loads the appropriate policy into the memory and returns back a vector of rules and the policy type as specified in the file;
           * might also be the point to select AC type (ACL, ABAC etc)?? *
        */

        let policytype: u8 = 1;

        let vec_ids = [
            "aaa3b411006ad57868988f9fec672a31",
            "bbb3b411006ad57868988f9fec672a31",
            "aaabbb11006ad57868988f9fec672a31",
            "aaabbb11006ad57868988f9fec672a31",
        ];
        let vec_actions: Vec<Action> =
            vec![Action::Write, Action::Read, Action::Write, Action::Read];
        //let vec_perms = [];
        let mut policyrules: Vec<PolicyRule> = Vec::new();

        for i in 0..4 {
            policyrules.push(PolicyRule {
                sub: PolicySubject::Id(ZenohId::from_str(vec_ids[i]).unwrap()),
                ke: "test/thr".to_string(),
                action: vec_actions.get(i).unwrap().clone(),
                permission: true,
            });
        }
        //let policyruleset: PolicyRuleSet;

        println!("policy rules : {:?}", policyrules);

        Ok((policytype, policyrules))
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
