use rustc_hash::FxHashMap;
use zenoh_config::{AclConfig, Action, ConfigRule, Permission, PolicyRule, Subject};
use zenoh_keyexpr::keyexpr;
use zenoh_keyexpr::keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, KeBoxTree};
use zenoh_result::ZResult;

const NUMBER_OF_ACTIONS: usize = 4; //size of Action enum (small but might change)
const NUMBER_OF_PERMISSIONS: usize = 2; //size of permission enum (fixed)

pub struct PolicyForSubject(Vec<Vec<KeTreeRule>>); //vec of actions over vec of permission for tree of ke for this
pub struct PolicyMap(pub FxHashMap<i32, PolicyForSubject>); //index of subject_map instead of subject

type KeTreeRule = KeBoxTree<bool>;

pub struct PolicyEnforcer {
    pub(crate) acl_enabled: bool,
    pub(crate) default_deny: bool,
    pub(crate) subject_map: Option<FxHashMap<Subject, i32>>, //should have all attribute names
    pub(crate) policy_list: Option<PolicyMap>,
}

#[derive(Debug, Clone)]

pub struct PolicyInformation {
    subject_map: FxHashMap<Subject, i32>,
    policy_rules: Vec<PolicyRule>,
}

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
            None => log::error!("acl config was not setup properly"),
        }
        match acl_config.blacklist {
            Some(val) => self.default_deny = val,
            None => log::error!("error default_deny not setup"),
        }
        if self.acl_enabled {
            match acl_config.rules {
                Some(policy_list) => {
                    let policy_information = self.policy_information_point(policy_list)?;

                    let subject_map = policy_information.subject_map;
                    let mut main_policy: PolicyMap = PolicyMap(FxHashMap::default());
                    //first initialize the vector of vectors (needed to maintain the indices)
                    for (_, index) in &subject_map {
                        let mut rule: PolicyForSubject = PolicyForSubject(Vec::new());
                        for _i in 0..NUMBER_OF_ACTIONS {
                            let mut action_rule: Vec<KeTreeRule> = Vec::new();
                            for _j in 0..NUMBER_OF_PERMISSIONS {
                                let permission_rule = KeTreeRule::new();
                                //
                                action_rule.push(permission_rule);
                            }
                            rule.0.push(action_rule);
                        }
                        main_policy.0.insert(*index, rule);
                    }

                    for rule in policy_information.policy_rules {
                        //add key-expression values to the ketree as per the policy rules

                        let index = subject_map.get(&rule.subject).unwrap();
                        main_policy.0.get_mut(index).unwrap().0[rule.action as usize]
                            [rule.permission as usize]
                            .insert(keyexpr::new(&rule.key_expr)?, true);
                    }
                    //add to the policy_enforcer
                    self.policy_list = Some(main_policy);
                    self.subject_map = Some(subject_map);
                }
                None => log::error!("no policy list was specified"),
            }
        }
        Ok(())
    }
    pub fn policy_information_point(
        &self,
        config_rule_set: Vec<ConfigRule>,
    ) -> ZResult<PolicyInformation> {
        /*
           get the list of policies from the config policymap
           convert them into the subject format for the vec of rules
           send the vec as part of policy information
           also take the subject values to create the subject_map and pass that as part of poliy infomration
        */
        //we need to convert the vector sets of rules into individual rules for each subject, key-expr, action, permission
        let mut policy_rules: Vec<PolicyRule> = Vec::new();
        for config_rule in config_rule_set {
            for subject in &config_rule.interface {
                for action in &config_rule.action {
                    for key_expr in &config_rule.key_expr {
                        policy_rules.push(PolicyRule {
                            subject: Subject::Interface(subject.clone()),
                            key_expr: key_expr.clone(),
                            action: action.clone(),
                            permission: config_rule.permission.clone(),
                        })
                    }
                }
            }
        }
        //create subject map
        let mut subject_map = FxHashMap::default();
        let mut counter = 1; //starting at 1 since 0 is initialized value in policy_check and should not match anything
        for rule in policy_rules.iter() {
            subject_map.insert(rule.subject.clone(), counter);
            counter += 1;
        }
        Ok(PolicyInformation {
            subject_map,
            policy_rules,
        })
    }

    /*
       checks each msg against the ACL ruleset for allow/deny
    */

    pub fn policy_decision_point(
        &self,
        subject: i32,
        action: Action,
        key_expr: &str, //String,
    ) -> ZResult<bool> {
        match &self.policy_list {
            Some(policy_map) => {
                let ps = policy_map.0.get(&subject).unwrap();
                let perm_vec = &ps.0[action as usize];

                //check for deny

                let deny_result = perm_vec[Permission::Deny as usize]
                    .nodes_including(keyexpr::new(&key_expr).unwrap())
                    .count();
                if deny_result != 0 {
                    return Ok(false);
                }

                //check for allow

                let allow_result = perm_vec[Permission::Allow as usize]
                    .nodes_including(keyexpr::new(&key_expr).unwrap())
                    .count();
                Ok(allow_result != 0)
            }
            None => Ok(false),
        }
    }
}
