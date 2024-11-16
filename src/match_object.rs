// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use serde_json::Value;
use envsubst;

pub fn contains(input: &Value, compare: &Value, env: &HashMap<String, String>) -> bool {
    match (input, compare) {
        (Value::Object(map_input), Value::Object(map_compare)) => {
            for (key, val_compare) in map_compare {
                match map_input.get(key) {
                    Some(val_input) => {
                        if !contains(val_input, val_compare, env) {
                            return false;
                        }
                    }
                    None => return false,
                }
            }
            true
        }
        (Value::Array(arr_input), Value::Array(arr_compare)) => {
            for val_compare in arr_compare {
                let mut found = false;
                for val_input in arr_input {
                    if contains(val_input, val_compare, env) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return false;
                }
            }
            true
        }
        (Value::String(s_input), Value::String(s_compare)) => {
            match envsubst::substitute(s_compare, env) {
                Ok(s_compare_subst) => s_input == &s_compare_subst,
                Err(_) => s_input == s_compare,
            }
        }
        _ => input == compare
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use serde_json::json;

    #[rstest]
    #[case(
        json!({"a": 1, "b": 2}),
        json!({"a": 1}),
        true
    )]
    #[case(
        json!({"a": 1, "b": {"c": 3, "d": 4}}),
        json!({"b": {"c": 3}}),
        true
    )]
    #[case(
        json!({"a": [1, 2, 3], "b": 4}),
        json!({"a": [2]}),
        true
    )]
    #[case(
        json!({"a": [1, 2, 3], "b": 4}),
        json!({"a": [4]}),
        false
    )]
    #[case(
        json!(["apple", "banana", "cherry"]),
        json!(["banana"]),
        true
    )]
    #[case(
        json!(["apple", "banana", "cherry"]),
        json!(["banana", "date"]),
        false
    )]
    #[case(
        json!(null),
        json!(null),
        true
    )]
    #[case(
        json!(null),
        json!(1),
        false
    )]
    #[case(
        json!(1),
        json!(1),
        true
    )]
    #[case(
        json!(1),
        json!(2),
        false
    )]
    #[case(
        json!({"a": {"b": {"c": 1}}}),
        json!({"a": {"b": {"c": 1}}}),
        true
    )]
    #[case(
        json!({"a": {"b": {"c": 1, "d": 2}}}),
        json!({"a": {"b": {"c": 1}}}),
        true
    )]
    #[case(
        json!({"a": {"b": {"c": 1}}}),
        json!({"a": {"b": {"c": 2}}}),
        false
    )]
    #[case(
        json!({"a": [ {"b": 1}, {"b": 2}, {"b": 3} ]}),
        json!({"a": [ {"b": 2} ]}),
        true
    )]
    #[case(
        json!({"a": [ {"b": 1}, {"b": 2}, {"b": 3} ]}),
        json!({"a": [ {"b": 4} ]}),
        false
    )]
    #[case(
        json!({"a": {"b": [1, 2, 3]}}),
        json!({"a": {"b": [2, 3]}}),
        true
    )]
    #[case(
        json!({"a": {"b": [1, 2, 3]}}),
        json!({"a": {"b": [4]}}),
        false
    )]
    #[case(
        json!({"a": {"b": 1}}),
        json!({"a": {"b": 2}}),
        false
    )]
    #[case(
        json!({"a": [ { "c": "foo", "b": false} ]}),
        json!({"a": [ { "c": "foo", "b": true} ]}),
        false
    )]
    #[case(
        json!({"a": [ { "c": "foo", "b": true} ]}),
        json!({"a": [ { "c": "foo", "b": true} ]}),
        true
    )]
    #[case(
        json!({
            "spec": "some_spec",
            "status": {
              "conditions": [
                {
                  "lastProbeTime": null,
                  "lastTransitionTime": "2024-11-02T15:49:19Z",
                  "status": "True",
                  "type": "Ready"
                },
                {
                  "lastProbeTime": null,
                  "lastTransitionTime": "2024-11-02T15:49:19Z",
                  "status": "True",
                  "type": "ContainersReady"
                },
                {
                  "lastProbeTime": null,
                  "lastTransitionTime": "2024-11-02T15:49:14Z",
                  "status": "True",
                  "type": "PodScheduled"
                }
              ],
              "containerStatuses": [
                {
                  "containerID": "docker://1b9a81f558cc5980a152a554ba6b03866062e91958c0af72c8290b62633dd2f7",
                  "image": "eclipse-mosquitto:2",
                  "state": {
                    "running": {
                      "startedAt": "2024-11-02T15:49:19Z"
                    }
                  },
                  "volumeMounts": [
                    {
                      "mountPath": "/mosquitto/config",
                      "name": "configuration"
                    },
                    {
                      "mountPath": "/var/run/secrets/kubernetes.io/serviceaccount",
                      "name": "kube-api-access-dtsbh",
                      "readOnly": true,
                      "recursiveReadOnly": "Disabled"
                    }
                  ]
                }
              ]
            }
        }),
        json!({
            "status": {
              "conditions": [
                {
                  "type": "Ready",
                  "status": "True"
                }
              ]
            }
        }),
        true
    )]
    #[case(
        json!({
            "spec": "some_spec",
            "status": {
              "conditions": [
                {
                  "lastProbeTime": null,
                  "lastTransitionTime": "2024-11-02T15:49:19Z",
                  "status": "False",
                  "type": "Ready"
                },
                {
                  "lastProbeTime": null,
                  "lastTransitionTime": "2024-11-02T15:49:19Z",
                  "status": "True",
                  "type": "ContainersReady"
                },
                {
                  "lastProbeTime": null,
                  "lastTransitionTime": "2024-11-02T15:49:14Z",
                  "status": "True",
                  "type": "PodScheduled"
                }
              ],
              "containerStatuses": [
                {
                  "containerID": "docker://1b9a81f558cc5980a152a554ba6b03866062e91958c0af72c8290b62633dd2f7",
                  "image": "eclipse-mosquitto:2",
                  "state": {
                    "running": {
                      "startedAt": "2024-11-02T15:49:19Z"
                    }
                  },
                  "volumeMounts": [
                    {
                      "mountPath": "/mosquitto/config",
                      "name": "configuration"
                    },
                    {
                      "mountPath": "/var/run/secrets/kubernetes.io/serviceaccount",
                      "name": "kube-api-access-dtsbh",
                      "readOnly": true,
                      "recursiveReadOnly": "Disabled"
                    }
                  ]
                }
              ]
            }
        }),
        json!({
            "status": {
              "conditions": [
                {
                  "type": "Ready",
                  "status": "True"
                }
              ]
            }
        }),
        false
    )]
    fn test_contains(#[case] input: Value, #[case] compare: Value, #[case] expected: bool) {
        let result = contains(&input, &compare, &HashMap::new());
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case(
        json!({"a": "123", "b": "234"}),
        json!({"a": "123"}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        true
    )]
    #[case(
        json!({"a": "12${ENV3}3", "b": "234"}),
        json!({"a": "12${ENV3}3"}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        true
    )]
    #[case(
        json!({"a": "129993", "b": "234"}),
        json!({"a": "12${ENV1}3"}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        true
    )]
    #[case(
        json!({"a": "126663", "b": "234"}),
        json!({"a": "12${ENV2}3"}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        true
    )]
    #[case(
        json!({"a": "129993", "b": "234"}),
        json!({"a": "12$ENV13"}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        false
    )]
    #[case(
        json!({"a": "126663", "b": "234"}),
        json!({"a": "12$ENV23"}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        false
    )]


    #[case(
        json!({"a": [ "234", "123" ], "b": "234"}),
        json!({"a": [ "123" ]}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        true
    )]
    #[case(
        json!({"a": [ "234", "129993" ], "b": "234"}),
        json!({"a": [ "12${ENV1}3" ]}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        true
    )]
    #[case(
        json!({"a": [ "234", "126663" ], "b": "234"}),
        json!({"a": [ "12${ENV2}3" ]}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        true
    )]
    #[case(
        json!({"a": [ "234", "12${ENV3}3" ], "b": "234"}),
        json!({"a": [ "12${ENV3}3" ]}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        true
    )]
    #[case(
        json!({"a": [ "234", "129993" ], "b": "234"}),
        json!({"a": [ "12$ENV13" ]}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        false
    )]
    #[case(
        json!({"a": [ "234", "126663" ], "b": "234"}),
        json!({"a": [ "12$ENV23" ]}),
        HashMap::from([("ENV1".to_string(), "999".to_string()), ("ENV2".to_string(), "666".to_string())]),
        false
    )]
    fn test_contains_with_env(#[case] input: Value, #[case] compare: Value, #[case] env: HashMap<String, String>, #[case] expected: bool) {
        let result = contains(&input, &compare, &env);
        assert_eq!(result, expected);
    }
}
