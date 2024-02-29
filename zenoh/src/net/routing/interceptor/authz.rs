use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use zenoh_config::AclConfig;
use zenoh_keyexpr::keyexpr;
use zenoh_keyexpr::keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, KeBoxTree};
use zenoh_result::ZResult;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]

pub enum Action {
    Pub,
    Sub,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]

pub enum Permission {
    Allow,
    Deny,
}

pub struct PolicyForSubject(Vec<Vec<KeTreeRule>>); //vec of actions over vec of permission for tree of ke for this
pub struct PolicyMap(pub FxHashMap<i32, PolicyForSubject>); //index of subject_map instead of subject

#[derive(Deserialize, Debug)]
pub struct GetPolicy {
    policy_definition: String,
    ruleset: Vec<AttributeRules>,
}
type KeTreeRule = KeBoxTree<bool>;

pub struct PolicyEnforcer {
    pub(crate) acl_enabled: bool,
    pub(crate) default_deny: bool,
    pub(crate) subject_map: Option<FxHashMap<Subject, i32>>, //should have all attribute names
    pub(crate) policy_list: Option<PolicyMap>,
}

#[derive(Debug, Clone)]

pub struct PolicyInformation {
    policy_definition: String,
    attribute_list: Vec<String>, //list of attribute names in string
    subject_map: FxHashMap<Subject, i32>,
    policy_rules: Vec<AttributeRules>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AttributeRules {
    attribute: String,
    rules: Vec<AttributeRule>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AttributeRule {
    subject: Subject,
    ke: String,
    action: Action,
    permission: Permission,
}
use zenoh_config::ZenohId;

#[derive(Serialize, Debug, Deserialize, Eq, PartialEq, Hash, Clone)]
#[serde(untagged)]
pub enum Subject {
    UserId(ZenohId),
    NetworkInterface(String),
}
#[derive(Debug)]
pub struct RequestInfo {
    pub sub: Vec<Subject>,
    pub ke: String,
    pub action: Action,
}

const ACTION_LENGTH: usize = 2;
const PERMISSION_LENGTH: usize = 2;

impl PolicyEnforcer {
    pub fn new() -> PolicyEnforcer {
        PolicyEnforcer {
            acl_enabled: true,
            default_deny: true,
            subject_map: None,
            policy_list: None,
        }
    }

    pub fn init(&mut self, acl_config: AclConfig) -> ZResult<()> {
        //returns Ok() for all good else returns Error
        //insert values into the enforcer from the config file
        match acl_config.enabled {
            Some(val) => self.acl_enabled = val,
            None => log::error!("acl config not setup"),
        }
        match acl_config.default_deny {
            Some(val) => self.default_deny = val,
            None => log::error!("error default_deny not setup"),
        }
        if self.acl_enabled {
            match acl_config.policy_list {
                Some(policy_list) => {
                    let policy_information = self.policy_information_point(policy_list)?;

                    self.subject_map = Some(policy_information.subject_map.clone());
                    let mut main_policy: PolicyMap = PolicyMap(FxHashMap::default());

                    //first initialize the vector of vectors (needed to maintain the indices)
                    let subject_map = policy_information.subject_map.clone();
                    for (_, index) in subject_map {
                        let mut rule: PolicyForSubject = PolicyForSubject(Vec::new());
                        for _i in 0..ACTION_LENGTH {
                            let mut action_rule: Vec<KeTreeRule> = Vec::new();
                            for _j in 0..PERMISSION_LENGTH {
                                let permission_rule = KeTreeRule::new();
                                //
                                action_rule.push(permission_rule);
                            }
                            rule.0.push(action_rule);
                        }
                        main_policy.0.insert(index, rule);
                    }

                    for rules in policy_information.policy_rules {
                        for rule in rules.rules {
                            //add values to the ketree as per the rules
                            //get the subject index
                            let index = policy_information.subject_map.get(&rule.subject).unwrap();
                            main_policy.0.get_mut(index).unwrap().0[rule.action as usize]
                                [rule.permission as usize]
                                .insert(keyexpr::new(&rule.ke)?, true);
                        }
                    }
                    //add to the policy_enforcer
                    self.policy_list = Some(main_policy);
                }
                None => log::error!("no policy list was specified"),
            }
        }
        Ok(())
    }
    pub fn policy_information_point(
        &self,
        policy_list: zenoh_config::PolicyList,
    ) -> ZResult<PolicyInformation> {
        let value = serde_json::to_value(&policy_list).unwrap();
        let policy_list_info: GetPolicy = serde_json::from_value(value)?;
        let enforced_attributes = policy_list_info
            .policy_definition
            .split(' ')
            .collect::<Vec<&str>>();

        let complete_ruleset = policy_list_info.ruleset;
        let mut attribute_list: Vec<String> = Vec::new();
        let mut policy_rules: Vec<AttributeRules> = Vec::new();
        let mut subject_map = FxHashMap::default();
        let mut counter = 1; //starting at 1 since 0 is default value in policy_check and should not match anything
        for attr_rule in complete_ruleset.iter() {
            if enforced_attributes.contains(&attr_rule.attribute.as_str()) {
                attribute_list.push(attr_rule.attribute.clone());
                policy_rules.push(attr_rule.clone());
                for rule in attr_rule.rules.clone() {
                    subject_map.insert(rule.subject, counter);
                    counter += 1;
                }
            }
        }

        let policy_definition = policy_list_info.policy_definition;

        Ok(PolicyInformation {
            policy_definition,
            attribute_list,
            subject_map,
            policy_rules,
        })
    }

    pub fn policy_decision_point(&self, subject: i32, act: Action, kexpr: String) -> ZResult<bool> {
        /*
           need to decide policy for proper handling of the edge cases
           what about default_deny vlaue from the policy file??
           if it is allow, the should be allowed if everything is NONE
        */
        match &self.policy_list {
            Some(policy_map) => {
                let ps = policy_map.0.get(&subject).unwrap();
                let perm_vec = &ps.0[act as usize];

                //check for deny

                let deny_result = perm_vec[Permission::Deny as usize]
                    .nodes_including(keyexpr::new(&kexpr).unwrap())
                    .count();
                if deny_result != 0 {
                    return Ok(false);
                }

                //check for allow

                let allow_result = perm_vec[Permission::Allow as usize]
                    .nodes_including(keyexpr::new(&kexpr).unwrap())
                    .count();
                Ok(allow_result != 0)
            }
            None => Ok(false),
        }
    }
}
