// Copyright 2024 Ole Kliemann
// SPDX-License-Identifier: Apache-2.0

use serde_json::Value;

pub fn contains(input: &Value, compare: &Value) -> bool {
    match (input, compare) {
        (Value::Object(map_input), Value::Object(map_compare)) => {
            for (key, val_compare) in map_compare {
                match map_input.get(key) {
                    Some(val_input) => {
                        if !contains(val_input, val_compare) {
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
                    if contains(val_input, val_compare) {
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
        _ => input == compare,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use rstest::rstest;

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
    fn test_contains(
        #[case] input: Value,
        #[case] compare: Value,
        #[case] expected: bool,
    ) {
        let result = contains(&input, &compare);
        assert_eq!(result, expected);
    }
}
