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

//! ⚠️ WARNING ⚠️
//!
//! This module is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](../zenoh/index.html)
use std::{collections::HashMap, net::Ipv4Addr};

use ahash::RandomState;
use itertools::Itertools;
use trie_rs::map::{Trie as TrieMap, TrieBuilder as TrieMapBuilder};
use zenoh_config::{
    AclConfig, AclConfigPolicyEntry, AclConfigRule, AclConfigSubjects, Action, InterceptorFlow,
    Permission, PolicyRule, Subject,
};
use zenoh_keyexpr::{
    keyexpr,
    keyexpr_tree::{IKeyExprTree, IKeyExprTreeMut, KeBoxTree},
};
use zenoh_result::ZResult;
use zenoh_util::net::get_interface_names_by_addr;
type PolicyForSubject = FlowPolicy;

type PolicyMap = HashMap<usize, PolicyForSubject, RandomState>;

#[derive(Debug, Clone)]
pub(crate) struct SubjectMap {
    inner: TrieMap<Subject, usize>,
}

impl SubjectMap {
    pub(crate) fn builder() -> SubjectMapBuilder {
        SubjectMapBuilder::new()
    }

    /// WIP, to replace with `get_`
    pub(crate) fn get(&self, subject: &Subject) -> Option<&usize> {
        unreachable!()
    }

    pub(crate) fn get_(&self, subjects: &[Subject]) -> Vec<(Vec<Subject>, &usize)> {
        let mut subjects = subjects.to_owned();
        subjects.sort_unstable();
        let mut res: Vec<(Vec<Subject>, &usize)> = vec![];
        for subject in subjects {
            let map_query: Vec<(Vec<Subject>, &usize)> =
                self.inner.predictive_search([subject]).collect();
            for (subject, id) in map_query {
                if subject.len() > 1 {
                    for s in &subject {
                        if subject.binary_search(s).is_err() {
                            continue;
                        }
                    }
                }
                res.push((subject, id))
            }
        }
        res
    }
}

impl Default for SubjectMap {
    fn default() -> Self {
        Self {
            inner: SubjectMapBuilder::new().builder.build(),
        }
    }
}

pub(crate) struct SubjectMapBuilder {
    builder: TrieMapBuilder<Subject, usize>,
    id_counter: usize,
}

impl SubjectMapBuilder {
    pub(crate) fn new() -> Self {
        Self {
            builder: TrieMapBuilder::new(),
            id_counter: 0,
        }
    }

    pub(crate) fn build(self) -> SubjectMap {
        SubjectMap {
            inner: self.builder.build(),
        }
    }

    /// Assumes subject contains at most one instance of each Subject variant
    pub(crate) fn insert(&mut self, subject: Vec<Subject>) -> usize {
        let mut subject = subject.clone();
        subject.sort_unstable();
        self.id_counter += 1;
        self.builder.insert(subject, self.id_counter);
        self.id_counter
    }
}

type KeTreeRule = KeBoxTree<bool>;

#[derive(Default)]
struct PermissionPolicy {
    allow: KeTreeRule,
    deny: KeTreeRule,
}

impl PermissionPolicy {
    #[allow(dead_code)]
    fn permission(&self, permission: Permission) -> &KeTreeRule {
        match permission {
            Permission::Allow => &self.allow,
            Permission::Deny => &self.deny,
        }
    }
    fn permission_mut(&mut self, permission: Permission) -> &mut KeTreeRule {
        match permission {
            Permission::Allow => &mut self.allow,
            Permission::Deny => &mut self.deny,
        }
    }
}
#[derive(Default)]
struct ActionPolicy {
    get: PermissionPolicy,
    put: PermissionPolicy,
    declare_subscriber: PermissionPolicy,
    declare_queryable: PermissionPolicy,
}

impl ActionPolicy {
    fn action(&self, action: Action) -> &PermissionPolicy {
        match action {
            Action::Get => &self.get,
            Action::Put => &self.put,
            Action::DeclareSubscriber => &self.declare_subscriber,
            Action::DeclareQueryable => &self.declare_queryable,
        }
    }
    fn action_mut(&mut self, action: Action) -> &mut PermissionPolicy {
        match action {
            Action::Get => &mut self.get,
            Action::Put => &mut self.put,
            Action::DeclareSubscriber => &mut self.declare_subscriber,
            Action::DeclareQueryable => &mut self.declare_queryable,
        }
    }
}

#[derive(Default)]
pub struct FlowPolicy {
    ingress: ActionPolicy,
    egress: ActionPolicy,
}

impl FlowPolicy {
    fn flow(&self, flow: InterceptorFlow) -> &ActionPolicy {
        match flow {
            InterceptorFlow::Ingress => &self.ingress,
            InterceptorFlow::Egress => &self.egress,
        }
    }
    fn flow_mut(&mut self, flow: InterceptorFlow) -> &mut ActionPolicy {
        match flow {
            InterceptorFlow::Ingress => &mut self.ingress,
            InterceptorFlow::Egress => &mut self.egress,
        }
    }
}

#[derive(Default, Debug)]
pub struct InterfaceEnabled {
    pub ingress: bool,
    pub egress: bool,
}

pub struct PolicyEnforcer {
    pub(crate) acl_enabled: bool,
    pub(crate) default_permission: Permission,
    pub(crate) subject_map: SubjectMap,
    pub(crate) policy_map: PolicyMap,
    pub(crate) interface_enabled: InterfaceEnabled,
}

#[derive(Debug, Clone)]
pub struct PolicyInformation {
    subject_map: SubjectMap,
    policy_rules: Vec<PolicyRule>,
}

impl PolicyEnforcer {
    pub fn new() -> PolicyEnforcer {
        PolicyEnforcer {
            acl_enabled: true,
            default_permission: Permission::Deny,
            subject_map: SubjectMap::default(),
            policy_map: PolicyMap::default(),
            interface_enabled: InterfaceEnabled::default(),
        }
    }

    /*
       initializes the policy_enforcer
    */
    pub fn init(&mut self, acl_config: &AclConfig) -> ZResult<()> {
        let mut_acl_config = acl_config.clone();
        self.acl_enabled = mut_acl_config.enabled;
        self.default_permission = mut_acl_config.default_permission;
        if self.acl_enabled {
            if let (Some(mut rules), Some(mut subjects), Some(policy)) = (
                mut_acl_config.rules,
                mut_acl_config.subjects,
                mut_acl_config.policy,
            ) {
                if rules.is_empty() || subjects.is_empty() || policy.is_empty() {
                    tracing::warn!("Access control rules/subjects/policy is empty in config file");
                    self.policy_map = PolicyMap::default();
                    self.subject_map = SubjectMap::default();
                    if self.default_permission == Permission::Deny {
                        self.interface_enabled = InterfaceEnabled {
                            ingress: true,
                            egress: true,
                        };
                    }
                } else {
                    // check for undefined values in rules and initialize them to defaults
                    for rule in rules.iter_mut() {
                        if rule.id.trim().is_empty() {
                            bail!("Found empty rule id in rules list");
                        }
                        if rule.flows.is_none() {
                            tracing::warn!("Rule '{}' flows list is not set. Setting it to both Ingress and Egress", rule.id);
                            rule.flows =
                                Some([InterceptorFlow::Ingress, InterceptorFlow::Egress].into());
                        }
                    }
                    // check for undefined values in subjects and initialize them to defaults
                    for subject in subjects.iter_mut() {
                        if subject.id.trim().is_empty() {
                            bail!("Found empty subject id in subjects list");
                        }
                        if subject.interfaces.is_none() {
                            if subject.cert_common_names.is_none() && subject.usernames.is_none() {
                                tracing::warn!(
                                    "Subject '{}' is empty. Setting it to wildcard (all network interfaces)",
                                    subject.id
                                );
                                subject.interfaces = Some(get_interface_names_by_addr(
                                    Ipv4Addr::UNSPECIFIED.into(),
                                )?);
                            } else {
                                subject.interfaces = Some(vec![]);
                            }
                        }
                        if subject.usernames.is_none() {
                            subject.usernames = Some(Vec::new());
                        }
                        if subject.cert_common_names.is_none() {
                            subject.cert_common_names = Some(Vec::new());
                        }
                    }
                    let policy_information =
                        self.policy_information_point(subjects, rules, policy)?;

                    let mut main_policy: PolicyMap = PolicyMap::default();
                    for rule in policy_information.policy_rules {
                        let subject_policy = main_policy.entry(rule.subject_id).or_default();
                        subject_policy
                            .flow_mut(rule.flow)
                            .action_mut(rule.action)
                            .permission_mut(rule.permission)
                            .insert(keyexpr::new(&rule.key_expr)?, true);

                        if self.default_permission == Permission::Deny {
                            self.interface_enabled = InterfaceEnabled {
                                ingress: true,
                                egress: true,
                            };
                        } else {
                            match rule.flow {
                                InterceptorFlow::Ingress => {
                                    self.interface_enabled.ingress = true;
                                }
                                InterceptorFlow::Egress => {
                                    self.interface_enabled.egress = true;
                                }
                            }
                        }
                    }
                    self.policy_map = main_policy;
                    self.subject_map = policy_information.subject_map;
                }
            } else {
                tracing::warn!("One of access control rules/subjects/policy lists is not provided");
            }
        }
        Ok(())
    }

    /*
       converts the sets of rules from config format into individual rules for each subject, key-expr, action, permission
    */
    pub fn policy_information_point(
        &self,
        mut subjects: Vec<AclConfigSubjects>,
        rules: Vec<AclConfigRule>,
        policy: Vec<AclConfigPolicyEntry>,
    ) -> ZResult<PolicyInformation> {
        let mut policy_rules: Vec<PolicyRule> = Vec::new();
        let mut rule_map: HashMap<String, AclConfigRule, RandomState> =
            HashMap::with_hasher(RandomState::default());
        let mut subject_id_map: HashMap<String, Vec<usize>, RandomState> =
            HashMap::with_hasher(RandomState::default());
        let mut subject_map_builder = SubjectMap::builder();

        // validate rules config and insert them in hashmaps
        for config_rule in rules {
            if rule_map.contains_key(&config_rule.id) {
                bail!(
                    "Rule id must be unique: id '{}' is repeated",
                    config_rule.id
                );
            }
            // Config validation
            let mut validation_err = String::new();
            if config_rule.actions.is_empty() {
                validation_err.push_str("ACL config actions list is empty. ");
            }
            if config_rule.flows.as_ref().unwrap().is_empty() {
                validation_err.push_str("ACL config flows list is empty. ");
            }
            if config_rule.key_exprs.is_empty() {
                validation_err.push_str("ACL config key_exprs list is empty. ");
            }
            if !validation_err.is_empty() {
                bail!("Rule '{}' is malformed: {}", config_rule.id, validation_err);
            }
            for key_expr in config_rule.key_exprs.iter() {
                if key_expr.trim().is_empty() {
                    bail!("Found empty key expression in rule '{}'", config_rule.id);
                }
            }
            rule_map.insert(config_rule.id.clone(), config_rule);
        }

        for config_subject in subjects.iter_mut() {
            if subject_id_map.contains_key(&config_subject.id) {
                bail!(
                    "Subject id must be unique: id '{}' is repeated",
                    config_subject.id
                );
            }
            // validate subject config fields
            let interfaces = config_subject.interfaces.as_ref().unwrap();
            let cert_common_names = config_subject.cert_common_names.as_ref().unwrap();
            let usernames = config_subject.usernames.as_ref().unwrap();
            if interfaces.is_empty() && cert_common_names.is_empty() && usernames.is_empty() {
                bail!("Subject '{}' is malformed: one of interfaces, cert_common_names or usernames lists must be specified and not empty", config_subject.id)
            }
            // create ACL individual subjects
            let mut subject_interfaces: Vec<Option<Subject>> = vec![];
            let mut subject_ccns: Vec<Option<Subject>> = vec![];
            let mut subject_usernames: Vec<Option<Subject>> = vec![];
            if interfaces.is_empty() {
                subject_interfaces.push(None);
            } else {
                for face in interfaces {
                    if face.trim().is_empty() {
                        bail!(
                            "Found empty interface value in subject '{}'",
                            config_subject.id
                        );
                    }
                    subject_interfaces.push(Some(Subject::Interface(face.into())));
                }
            }
            if cert_common_names.is_empty() {
                subject_ccns.push(None);
            } else {
                for cert_common_name in cert_common_names {
                    if cert_common_name.trim().is_empty() {
                        bail!(
                            "Found empty cert_common_name value in subject '{}'",
                            config_subject.id
                        );
                    }
                    subject_ccns.push(Some(Subject::CertCommonName(cert_common_name.into())));
                }
            }
            if usernames.is_empty() {
                subject_usernames.push(None);
            } else {
                for username in usernames {
                    if username.trim().is_empty() {
                        bail!(
                            "Found empty username value in subject '{}'",
                            config_subject.id
                        );
                    }
                    subject_usernames.push(Some(Subject::Username(username.into())));
                }
            }
            // create ACL subject combinations
            let subject_combination_ids = subject_interfaces
                .iter()
                .cartesian_product(&subject_ccns)
                .cartesian_product(&subject_usernames)
                .filter_map(|((face, ccn), usr)| {
                    let mut combination: Vec<Subject> = vec![];
                    // NOTE: order doesn't matter since the insert function will sort
                    face.is_some()
                        .then(|| combination.push(face.clone().unwrap()));
                    ccn.is_some()
                        .then(|| combination.push(ccn.clone().unwrap()));
                    usr.is_some()
                        .then(|| combination.push(usr.clone().unwrap()));
                    if !combination.is_empty() {
                        return Some(subject_map_builder.insert(combination));
                    }
                    None
                })
                .collect();
            subject_id_map.insert(config_subject.id.clone(), subject_combination_ids);
        }
        // finally, handle policy content
        for (entry_id, entry) in policy.iter().enumerate() {
            // validate policy config lists
            if entry.rules.is_empty() || entry.subjects.is_empty() {
                bail!(
                    "Policy entry #{} is malformed: empty subjects or rules list",
                    entry_id + 1
                );
            }
            for subject_config_id in &entry.subjects {
                if !subject_id_map.contains_key(subject_config_id) {
                    bail!(
                        "Subject '{}' in policy entry #{} does not exist in subjects list",
                        subject_config_id,
                        entry_id
                    )
                }
            }
            // Create PolicyRules
            for rule_id in &entry.rules {
                let rule = rule_map.get(rule_id).ok_or(zerror!(
                    "Rule '{}' in policy entry #{} does not exist in rules list",
                    rule_id,
                    entry_id
                ))?;
                for subject_config_id in &entry.subjects {
                    let subject_combination_ids = subject_id_map.get(subject_config_id).unwrap();
                    for subject_id in subject_combination_ids {
                        for flow in rule.flows.as_ref().unwrap() {
                            for action in &rule.actions {
                                for key_expr in &rule.key_exprs {
                                    policy_rules.push(PolicyRule {
                                        subject_id: *subject_id,
                                        key_expr: key_expr.clone(),
                                        action: *action,
                                        permission: rule.permission,
                                        flow: *flow,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(PolicyInformation {
            subject_map: subject_map_builder.build(),
            policy_rules,
        })
    }

    /**
     * Check each msg against the ACL ruleset for allow/deny
     */
    pub fn policy_decision_point(
        &self,
        subject: usize,
        flow: InterceptorFlow,
        action: Action,
        key_expr: &str,
    ) -> ZResult<Permission> {
        let policy_map = &self.policy_map;
        if policy_map.is_empty() {
            return Ok(self.default_permission);
        }
        match policy_map.get(&subject) {
            Some(single_policy) => {
                let deny_result = single_policy
                    .flow(flow)
                    .action(action)
                    .deny
                    .nodes_including(keyexpr::new(&key_expr)?)
                    .count();
                if deny_result != 0 {
                    return Ok(Permission::Deny);
                }
                if self.default_permission == Permission::Allow {
                    Ok(Permission::Allow)
                } else {
                    let allow_result = single_policy
                        .flow(flow)
                        .action(action)
                        .allow
                        .nodes_including(keyexpr::new(&key_expr)?)
                        .count();

                    if allow_result != 0 {
                        Ok(Permission::Allow)
                    } else {
                        Ok(Permission::Deny)
                    }
                }
            }
            None => Ok(self.default_permission),
        }
    }
}
