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
use std::{
    collections::{HashMap, HashSet},
    net::Ipv4Addr,
};

use ahash::RandomState;
use trie_rs::map::{Trie as TrieMap, TrieBuilder as TrieMapBuilder};
use zenoh_config::{
    AclConfig, AclConfigRules, Action, InterceptorFlow, Permission, PolicyRule, Subject,
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

    pub(crate) fn get(&self, subject: &Subject) -> Option<&usize> {
        todo!()
    }
}

impl Default for SubjectMap {
    fn default() -> Self {
        Self {
            inner: SubjectMapBuilder::new().inner.build(),
        }
    }
}

pub(crate) struct SubjectMapBuilder {
    inner: TrieMapBuilder<Subject, usize>,
    subject_set: HashSet<Vec<Subject>, RandomState>,
    id_counter: usize,
}

impl SubjectMapBuilder {
    pub(crate) fn new() -> Self {
        Self {
            inner: TrieMapBuilder::new(),
            subject_set: HashSet::with_hasher(RandomState::default()),
            id_counter: 0,
        }
    }

    pub(crate) fn build(self) -> SubjectMap {
        SubjectMap {
            inner: self.inner.build(),
        }
    }

    pub(crate) fn insert_single_subject(&mut self, subject: Subject) -> Option<usize> {
        if matches!(&subject, Subject::None) || !self.subject_set.insert(vec![subject.clone()]) {
            return None;
        }
        self.id_counter += 1;
        match subject {
            Subject::Interface(_) => self.inner.push([subject], self.id_counter),
            Subject::CertCommonName(_) => {
                self.inner.push([Subject::None, subject], self.id_counter)
            }
            Subject::Username(_) => self
                .inner
                .push([Subject::None, Subject::None, subject], self.id_counter),
            Subject::None => {}
        }
        Some(self.id_counter)
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
            if let Some(mut rules) = mut_acl_config.rules {
                if rules.is_empty() {
                    tracing::warn!("Access control rules are empty in config file");
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
                    for (rule_offset, rule) in rules.iter_mut().enumerate() {
                        if rule.interfaces.is_none() {
                            tracing::warn!("ACL config interfaces list is empty. Applying rule #{} to all network interfaces", rule_offset);
                            rule.interfaces =
                                Some(get_interface_names_by_addr(Ipv4Addr::UNSPECIFIED.into())?);
                        }
                        if rule.flows.is_none() {
                            tracing::warn!("ACL config flows list is empty. Applying rule #{} to both Ingress and Egress flows", rule_offset);
                            rule.flows =
                                Some([InterceptorFlow::Ingress, InterceptorFlow::Egress].into());
                        }
                        if rule.usernames.is_none() {
                            rule.usernames = Some(Vec::new());
                        }
                        if rule.cert_common_names.is_none() {
                            rule.cert_common_names = Some(Vec::new());
                        }
                    }
                    let policy_information = self.policy_information_point(&rules)?;
                    let subject_map = policy_information.subject_map;
                    let mut main_policy: PolicyMap = PolicyMap::default();

                    for rule in policy_information.policy_rules {
                        if let Some(index) = subject_map.get(&rule.subject) {
                            let single_policy = main_policy.entry(*index).or_default();
                            single_policy
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
                        };
                    }
                    self.policy_map = main_policy;
                    self.subject_map = subject_map;
                }
            } else {
                tracing::warn!("Access control rules are empty in config file");
            }
        }
        Ok(())
    }

    /*
       converts the sets of rules from config format into individual rules for each subject, key-expr, action, permission
    */
    pub fn policy_information_point(
        &self,
        config_rule_set: &Vec<AclConfigRules>,
    ) -> ZResult<PolicyInformation> {
        let mut policy_rules: Vec<PolicyRule> = Vec::new();
        for config_rule in config_rule_set {
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
                bail!("{}", validation_err);
            }

            // At least one must not be empty
            let mut subject_validation_err: usize = 0;
            validation_err = String::new();

            if config_rule.interfaces.as_ref().unwrap().is_empty() {
                subject_validation_err += 1;
                validation_err.push_str("ACL config interfaces list is empty. ");
            }
            if config_rule.cert_common_names.as_ref().unwrap().is_empty() {
                subject_validation_err += 1;
                validation_err.push_str("ACL config certificate common names list is empty. ");
            }
            if config_rule.usernames.as_ref().unwrap().is_empty() {
                subject_validation_err += 1;
                validation_err.push_str("ACL config usernames list is empty. ");
            }

            if subject_validation_err == 3 {
                bail!("{}", validation_err);
            }

            for subject in config_rule.interfaces.as_ref().unwrap() {
                if subject.trim().is_empty() {
                    bail!("found an empty interface value in interfaces list");
                }
                for flow in config_rule.flows.as_ref().unwrap() {
                    for action in &config_rule.actions {
                        for key_expr in &config_rule.key_exprs {
                            if key_expr.trim().is_empty() {
                                bail!("found an empty key-expression value in key_exprs list");
                            }
                            policy_rules.push(PolicyRule {
                                subject: Subject::Interface(subject.clone()),
                                key_expr: key_expr.clone(),
                                action: *action,
                                permission: config_rule.permission,
                                flow: *flow,
                            })
                        }
                    }
                }
            }
            for subject in config_rule.cert_common_names.as_ref().unwrap() {
                if subject.trim().is_empty() {
                    bail!("found an empty value in certificate common names list");
                }
                for flow in config_rule.flows.as_ref().unwrap() {
                    for action in &config_rule.actions {
                        for key_expr in &config_rule.key_exprs {
                            if key_expr.trim().is_empty() {
                                bail!("found an empty key-expression value in key_exprs list");
                            }
                            policy_rules.push(PolicyRule {
                                subject: Subject::CertCommonName(subject.clone()),
                                key_expr: key_expr.clone(),
                                action: *action,
                                permission: config_rule.permission,
                                flow: *flow,
                            })
                        }
                    }
                }
            }
            for subject in config_rule.usernames.as_ref().unwrap() {
                if subject.trim().is_empty() {
                    bail!("found an empty value in usernames list");
                }
                for flow in config_rule.flows.as_ref().unwrap() {
                    for action in &config_rule.actions {
                        for key_expr in &config_rule.key_exprs {
                            if key_expr.trim().is_empty() {
                                bail!("found an empty key-expression value in key_exprs list");
                            }
                            policy_rules.push(PolicyRule {
                                subject: Subject::Username(subject.clone()),
                                key_expr: key_expr.clone(),
                                action: *action,
                                permission: config_rule.permission,
                                flow: *flow,
                            })
                        }
                    }
                }
            }
        }
        let mut subject_map_builder = SubjectMap::builder();
        // Starting at 1 since 0 is the init value and should not match anything
        for rule in policy_rules.iter() {
            subject_map_builder.insert_single_subject(rule.subject.clone());
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
