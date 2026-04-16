# Test Coverage Audit

## Scope and Method
- Audit mode: static inspection only.
- Commands executed: file listing, grep/ripgrep searches, and file reads only. No tests/scripts/containers/package managers were executed.
- Primary evidence files:
  - Route inventory: `repo/backend/src/lib.rs`, `repo/backend/src/api/*.rs`, `repo/backend/src/api/dashboards.rs`
  - API tests: `repo/API_tests/*.sh`, `repo/backend/tests/api_routes_test.rs`
  - Unit tests: `repo/backend/src/**`, `repo/frontend/src/tests/*.rs`, `repo/frontend/src/layouts/main_layout.rs`
  - README: `repo/README.md`

## Project Type Detection
- README top declares: `Project Type: fullstack` (`repo/README.md:3`).
- Effective audit type: **fullstack**.

## Backend Endpoint Inventory
- Total endpoints discovered: **105**

- `DELETE /api/v1/attachments/<id>` (repo/backend/src/api/attachments.rs:219)
- `DELETE /api/v1/courses/<id>/prerequisites/<pid>` (repo/backend/src/api/courses.rs:229)
- `DELETE /api/v1/reports/schedules/<schedule_id>` (repo/backend/src/api/reports.rs:226)
- `DELETE /api/v1/users/<id>` (repo/backend/src/api/users.rs:372)
- `GET /api/v1/admin/config/<key>` (repo/backend/src/api/admin_config.rs:97)
- `GET /api/v1/admin/config/` (repo/backend/src/api/admin_config.rs:35)
- `GET /api/v1/admin/retention/<id>` (repo/backend/src/api/retention.rs:104)
- `GET /api/v1/admin/retention/` (repo/backend/src/api/retention.rs:79)
- `GET /api/v1/attachments/<id>/preview` (repo/backend/src/api/attachments.rs:196)
- `GET /api/v1/attachments/<id>` (repo/backend/src/api/attachments.rs:148)
- `GET /api/v1/attachments/?<parent_type>&<parent_id>` (repo/backend/src/api/attachments.rs:134)
- `GET /api/v1/audit-logs/?<actor_id>&<action>&<target_entity_type>&<target_entity_id>&<from>&<to>&<limit>` (repo/backend/src/api/audit_logs.rs:41)
- `GET /api/v1/audit-logs/export.csv?<actor_id>&<action>&<target_entity_type>&<target_entity_id>&<from>&<to>&<limit>` (repo/backend/src/api/audit_logs.rs:119)
- `GET /api/v1/audit-logs/verify-chain` (repo/backend/src/api/audit_logs.rs:196)
- `GET /api/v1/auth/me` (repo/backend/src/api/auth.rs:94)
- `GET /api/v1/checkins/?<section_id>&<limit>&<offset>` (repo/backend/src/api/checkins.rs:68)
- `GET /api/v1/checkins/retry-reasons` (repo/backend/src/api/checkins.rs:89)
- `GET /api/v1/courses/<id>/prerequisites` (repo/backend/src/api/courses.rs:188)
- `GET /api/v1/courses/<id>/versions/<vid>` (repo/backend/src/api/courses.rs:130)
- `GET /api/v1/courses/<id>/versions` (repo/backend/src/api/courses.rs:119)
- `GET /api/v1/courses/<id>` (repo/backend/src/api/courses.rs:94)
- `GET /api/v1/courses/?<department_id>&<limit>&<offset>` (repo/backend/src/api/courses.rs:59)
- `GET /api/v1/courses/export.csv` (repo/backend/src/api/courses.rs:265)
- `GET /api/v1/courses/export.xlsx` (repo/backend/src/api/courses.rs:279)
- `GET /api/v1/courses/template.csv` (repo/backend/src/api/courses.rs:245)
- `GET /api/v1/courses/template.xlsx` (repo/backend/src/api/courses.rs:255)
- `GET /api/v1/dashboards/course-popularity?<from>&<to>&<department_id>` (repo/backend/src/api/dashboards.rs:80)
- `GET /api/v1/dashboards/drop-rate?<from>&<to>&<department_id>` (repo/backend/src/api/dashboards.rs:90)
- `GET /api/v1/dashboards/dwell-time?<from>&<to>&<department_id>` (repo/backend/src/api/dashboards.rs:105)
- `GET /api/v1/dashboards/fill-rate?<from>&<to>&<department_id>` (repo/backend/src/api/dashboards.rs:85)
- `GET /api/v1/dashboards/foot-traffic?<from>&<to>&<department_id>` (repo/backend/src/api/dashboards.rs:100)
- `GET /api/v1/dashboards/instructor-workload?<from>&<to>&<department_id>` (repo/backend/src/api/dashboards.rs:95)
- `GET /api/v1/dashboards/interaction-quality?<from>&<to>&<department_id>` (repo/backend/src/api/dashboards.rs:110)
- `GET /api/v1/health` (repo/backend/src/api/health.rs:82)
- `GET /api/v1/journals/<id>/versions/<version_id>` (repo/backend/src/api/journals.rs:106)
- `GET /api/v1/journals/<id>/versions` (repo/backend/src/api/journals.rs:94)
- `GET /api/v1/journals/<id>` (repo/backend/src/api/journals.rs:55)
- `GET /api/v1/journals/?<limit>&<offset>` (repo/backend/src/api/journals.rs:37)
- `GET /api/v1/metrics/<id>/versions` (repo/backend/src/api/metrics.rs:88)
- `GET /api/v1/metrics/<id>` (repo/backend/src/api/metrics.rs:61)
- `GET /api/v1/metrics/?<limit>&<offset>` (repo/backend/src/api/metrics.rs:31)
- `GET /api/v1/reports/<id>/runs` (repo/backend/src/api/reports.rs:123)
- `GET /api/v1/reports/<id>/schedules` (repo/backend/src/api/reports.rs:180)
- `GET /api/v1/reports/<id>` (repo/backend/src/api/reports.rs:66)
- `GET /api/v1/reports/` (repo/backend/src/api/reports.rs:43)
- `GET /api/v1/reports/runs/<run_id>/download` (repo/backend/src/api/reports.rs:156)
- `GET /api/v1/reports/runs/<run_id>` (repo/backend/src/api/reports.rs:140)
- `GET /api/v1/roles/<id>` (repo/backend/src/api/roles.rs:96)
- `GET /api/v1/roles/` (repo/backend/src/api/roles.rs:39)
- `GET /api/v1/sections/<id>/versions` (repo/backend/src/api/sections.rs:126)
- `GET /api/v1/sections/<id>` (repo/backend/src/api/sections.rs:97)
- `GET /api/v1/sections/?<course_id>&<department_id>&<limit>&<offset>` (repo/backend/src/api/sections.rs:52)
- `GET /api/v1/sections/export.csv` (repo/backend/src/api/sections.rs:194)
- `GET /api/v1/sections/export.xlsx` (repo/backend/src/api/sections.rs:208)
- `GET /api/v1/sections/template.csv` (repo/backend/src/api/sections.rs:174)
- `GET /api/v1/sections/template.xlsx` (repo/backend/src/api/sections.rs:184)
- `GET /api/v1/teaching-resources/<id>/versions/<version_id>` (repo/backend/src/api/teaching_resources.rs:101)
- `GET /api/v1/teaching-resources/<id>/versions` (repo/backend/src/api/teaching_resources.rs:89)
- `GET /api/v1/teaching-resources/<id>` (repo/backend/src/api/teaching_resources.rs:51)
- `GET /api/v1/teaching-resources/?<limit>&<offset>` (repo/backend/src/api/teaching_resources.rs:33)
- `GET /api/v1/users/<id>` (repo/backend/src/api/users.rs:135)
- `GET /api/v1/users/` (repo/backend/src/api/users.rs:113)
- `GET /api/v1/users/me` (repo/backend/src/api/users.rs:103)
- `POST /api/v1/admin/artifact-backfill/` (repo/backend/src/api/artifact_backfill.rs:57)
- `POST /api/v1/admin/retention/<id>/execute` (repo/backend/src/api/retention.rs:166)
- `POST /api/v1/admin/retention/` (repo/backend/src/api/retention.rs:91)
- `POST /api/v1/admin/retention/execute` (repo/backend/src/api/retention.rs:138)
- `POST /api/v1/attachments/` (repo/backend/src/api/attachments.rs:58)
- `POST /api/v1/auth/login` (repo/backend/src/api/auth.rs:57)
- `POST /api/v1/auth/logout` (repo/backend/src/api/auth.rs:79)
- `POST /api/v1/checkins/<id>/retry` (repo/backend/src/api/checkins.rs:46)
- `POST /api/v1/checkins/` (repo/backend/src/api/checkins.rs:27)
- `POST /api/v1/courses/<id>/prerequisites` (repo/backend/src/api/courses.rs:208)
- `POST /api/v1/courses/<id>/versions/<vid>/approve` (repo/backend/src/api/courses.rs:156)
- `POST /api/v1/courses/<id>/versions/<vid>/publish` (repo/backend/src/api/courses.rs:171)
- `POST /api/v1/courses/` (repo/backend/src/api/courses.rs:83)
- `POST /api/v1/courses/import?<mode>` (repo/backend/src/api/courses.rs:301)
- `POST /api/v1/journals/<id>/versions/<version_id>/approve` (repo/backend/src/api/journals.rs:120)
- `POST /api/v1/journals/<id>/versions/<version_id>/publish` (repo/backend/src/api/journals.rs:134)
- `POST /api/v1/journals/` (repo/backend/src/api/journals.rs:69)
- `POST /api/v1/metrics/<id>/versions/<vid>/approve` (repo/backend/src/api/metrics.rs:101)
- `POST /api/v1/metrics/<id>/versions/<vid>/publish` (repo/backend/src/api/metrics.rs:116)
- `POST /api/v1/metrics/` (repo/backend/src/api/metrics.rs:49)
- `POST /api/v1/metrics/widgets/<widget_id>/verify` (repo/backend/src/api/metrics.rs:131)
- `POST /api/v1/reports/<id>/run` (repo/backend/src/api/reports.rs:98)
- `POST /api/v1/reports/<id>/schedules` (repo/backend/src/api/reports.rs:194)
- `POST /api/v1/reports/` (repo/backend/src/api/reports.rs:54)
- `POST /api/v1/sections/<id>/versions/<vid>/approve` (repo/backend/src/api/sections.rs:140)
- `POST /api/v1/sections/<id>/versions/<vid>/publish` (repo/backend/src/api/sections.rs:156)
- `POST /api/v1/sections/` (repo/backend/src/api/sections.rs:84)
- `POST /api/v1/sections/import?<mode>` (repo/backend/src/api/sections.rs:230)
- `POST /api/v1/teaching-resources/<id>/versions/<version_id>/approve` (repo/backend/src/api/teaching_resources.rs:115)
- `POST /api/v1/teaching-resources/<id>/versions/<version_id>/publish` (repo/backend/src/api/teaching_resources.rs:129)
- `POST /api/v1/teaching-resources/` (repo/backend/src/api/teaching_resources.rs:63)
- `POST /api/v1/users/` (repo/backend/src/api/users.rs:150)
- `PUT /api/v1/admin/config/<key>` (repo/backend/src/api/admin_config.rs:140)
- `PUT /api/v1/admin/retention/<id>` (repo/backend/src/api/retention.rs:118)
- `PUT /api/v1/courses/<id>` (repo/backend/src/api/courses.rs:106)
- `PUT /api/v1/journals/<id>` (repo/backend/src/api/journals.rs:80)
- `PUT /api/v1/metrics/<id>` (repo/backend/src/api/metrics.rs:74)
- `PUT /api/v1/reports/<id>` (repo/backend/src/api/reports.rs:80)
- `PUT /api/v1/reports/schedules/<schedule_id>` (repo/backend/src/api/reports.rs:210)
- `PUT /api/v1/sections/<id>` (repo/backend/src/api/sections.rs:111)
- `PUT /api/v1/teaching-resources/<id>` (repo/backend/src/api/teaching_resources.rs:74)
- `PUT /api/v1/users/<id>` (repo/backend/src/api/users.rs:262)

## API Test Mapping Table
| Endpoint | Covered | Test type | Test files | Evidence |
|---|---|---|---|---|
| `DELETE /api/v1/attachments/<id>` | yes | true no-mock HTTP | `repo/API_tests/library_attachment_upload_and_preview.sh` | `repo/API_tests/library_attachment_upload_and_preview.sh:2663` |
| `DELETE /api/v1/courses/<id>/prerequisites/<pid>` | yes | true no-mock HTTP | `repo/API_tests/academic_prerequisites.sh` | `repo/API_tests/academic_prerequisites.sh:669, repo/API_tests/academic_prerequisites.sh:676` |
| `DELETE /api/v1/reports/schedules/<schedule_id>` | yes | true no-mock HTTP | `repo/API_tests/report_schedule_lifecycle.sh` | `repo/API_tests/report_schedule_lifecycle.sh:4086` |
| `DELETE /api/v1/users/<id>` | yes | true no-mock HTTP | `repo/API_tests/user_crud.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/user_crud.sh:4647, repo/backend/tests/api_routes_test.rs:583` |
| `GET /api/v1/admin/config/<key>` | yes | true no-mock HTTP | `repo/API_tests/admin_config_write.sh` | `repo/API_tests/admin_config_write.sh:954` |
| `GET /api/v1/admin/config/` | yes | true no-mock HTTP | `repo/API_tests/admin_config.sh, repo/API_tests/admin_endpoint_protection.sh, repo/API_tests/auth_unauthenticated_access.sh` | `repo/API_tests/admin_config.sh:866, repo/API_tests/admin_config.sh:881, repo/API_tests/admin_endpoint_protection.sh:985, repo/API_tests/admin_endpoint_protection.sh:996, repo/API_tests/auth_unauthenticated_access.sh:8` |
| `GET /api/v1/admin/retention/<id>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2843` |
| `GET /api/v1/admin/retention/` | yes | true no-mock HTTP | `repo/API_tests/report_crypto_erase.sh, repo/API_tests/retention_execute.sh, repo/API_tests/retention_policy_update.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/report_crypto_erase.sh:3839, repo/API_tests/retention_execute.sh:4404, repo/API_tests/retention_policy_update.sh:4462, repo/API_tests/retention_policy_update.sh:4469, repo/API_tests/retention_policy_update.sh:4475, repo/API_tests/retention_policy_update.sh:4481, repo/API_tests/retention_policy_update.sh:4533, repo/backend/tests/api_routes_test.rs:668, repo/backend/tests/api_routes_test.rs:2829` |
| `GET /api/v1/attachments/<id>/preview` | yes | true no-mock HTTP | `repo/API_tests/library_attachment_upload_and_preview.sh, repo/API_tests/library_preview_unsupported_type.sh` | `repo/API_tests/library_attachment_upload_and_preview.sh:2645, repo/API_tests/library_preview_unsupported_type.sh:2993` |
| `GET /api/v1/attachments/<id>` | yes | true no-mock HTTP | `repo/API_tests/library_attachment_upload_and_preview.sh, repo/API_tests/library_preview_unsupported_type.sh` | `repo/API_tests/library_attachment_upload_and_preview.sh:2637, repo/API_tests/library_attachment_upload_and_preview.sh:2669, repo/API_tests/library_preview_unsupported_type.sh:2985` |
| `GET /api/v1/attachments/?<parent_type>&<parent_id>` | yes | true no-mock HTTP | `repo/API_tests/library_attachment_upload_and_preview.sh` | `repo/API_tests/library_attachment_upload_and_preview.sh:2628` |
| `GET /api/v1/audit-logs/?<actor_id>&<action>&<target_entity_type>&<target_entity_id>&<from>&<to>&<limit>` | yes | true no-mock HTTP | `repo/API_tests/admin_config_write.sh, repo/API_tests/audit_log_export.sh, repo/API_tests/audit_log_search.sh, repo/API_tests/audit_log_search_and_chain.sh, repo/API_tests/auth_unauthenticated_access.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/admin_config_write.sh:960, repo/API_tests/audit_log_export.sh:1386, repo/API_tests/audit_log_search.sh:1456, repo/API_tests/audit_log_search.sh:1474, repo/API_tests/audit_log_search.sh:1480, repo/API_tests/audit_log_search.sh:1504, repo/API_tests/audit_log_search_and_chain.sh:1522, repo/API_tests/auth_unauthenticated_access.sh:8, repo/backend/tests/api_routes_test.rs:158, repo/backend/tests/api_routes_test.rs:170` |
| `GET /api/v1/audit-logs/export.csv?<actor_id>&<action>&<target_entity_type>&<target_entity_id>&<from>&<to>&<limit>` | yes | true no-mock HTTP | `repo/API_tests/audit_log_export.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/audit_log_export.sh:1336, repo/API_tests/audit_log_export.sh:1403, repo/API_tests/audit_log_export.sh:1408, repo/API_tests/audit_log_export.sh:1417, repo/API_tests/audit_log_export.sh:1433, repo/backend/tests/api_routes_test.rs:374, repo/backend/tests/api_routes_test.rs:391, repo/backend/tests/api_routes_test.rs:431` |
| `GET /api/v1/audit-logs/verify-chain` | yes | true no-mock HTTP | `repo/API_tests/admin_endpoint_protection.sh, repo/API_tests/audit_log_search.sh, repo/API_tests/audit_log_search_and_chain.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/admin_endpoint_protection.sh:990, repo/API_tests/admin_endpoint_protection.sh:1001, repo/API_tests/audit_log_search.sh:1486, repo/API_tests/audit_log_search_and_chain.sh:1545, repo/backend/tests/api_routes_test.rs:210, repo/backend/tests/api_routes_test.rs:227` |
| `GET /api/v1/auth/me` | yes | true no-mock HTTP | `repo/API_tests/auth_login_success.sh, repo/API_tests/logout_revokes_session.sh, repo/API_tests/auth_unauthenticated_access.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/auth_login_success.sh:1642, repo/API_tests/logout_revokes_session.sh:3184, repo/API_tests/logout_revokes_session.sh:3195, repo/API_tests/auth_unauthenticated_access.sh:8, repo/backend/tests/api_routes_test.rs:568, repo/backend/tests/api_routes_test.rs:600` |
| `GET /api/v1/checkins/?<section_id>&<limit>&<offset>` | yes | true no-mock HTTP | `repo/API_tests/checkin_duplicate_blocked.sh, repo/API_tests/checkin_masking.sh` | `repo/API_tests/checkin_duplicate_blocked.sh:1717, repo/API_tests/checkin_masking.sh:1844, repo/API_tests/checkin_masking.sh:1854` |
| `GET /api/v1/checkins/retry-reasons` | yes | true no-mock HTTP | `repo/API_tests/checkin_retry_reasons_endpoint.sh` | `repo/API_tests/checkin_retry_reasons_endpoint.sh:2171` |
| `GET /api/v1/courses/<id>/prerequisites` | yes | true no-mock HTTP | `repo/API_tests/academic_prerequisites.sh` | `repo/API_tests/academic_prerequisites.sh:639` |
| `GET /api/v1/courses/<id>/versions/<vid>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:3013` |
| `GET /api/v1/courses/<id>/versions` | yes | true no-mock HTTP | `repo/API_tests/academic_course_happy_path.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/academic_course_happy_path.sh:158, repo/backend/tests/api_routes_test.rs:3004` |
| `GET /api/v1/courses/<id>` | yes | true no-mock HTTP | `repo/API_tests/academic_course_happy_path.sh` | `repo/API_tests/academic_course_happy_path.sh:107` |
| `GET /api/v1/courses/?<department_id>&<limit>&<offset>` | yes | true no-mock HTTP | `repo/API_tests/academic_export_scope.sh, repo/API_tests/academic_import_commit.sh, repo/API_tests/academic_import_dry_run.sh, repo/API_tests/encryption_field_masking.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/academic_export_scope.sh:303, repo/API_tests/academic_import_commit.sh:383, repo/API_tests/academic_import_commit.sh:414, repo/API_tests/academic_import_dry_run.sh:477, repo/API_tests/encryption_field_masking.sh:2470, repo/backend/tests/api_routes_test.rs:2995` |
| `GET /api/v1/courses/export.csv` | yes | true no-mock HTTP | `repo/API_tests/academic_export_scope.sh` | `repo/API_tests/academic_export_scope.sh:317` |
| `GET /api/v1/courses/export.xlsx` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2986` |
| `GET /api/v1/courses/template.csv` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2968` |
| `GET /api/v1/courses/template.xlsx` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2977` |
| `GET /api/v1/dashboards/course-popularity?<from>&<to>&<department_id>` | yes | true no-mock HTTP | `repo/API_tests/dashboard_date_filter.sh, repo/API_tests/dashboard_unauthorized_returns_403_or_401.sh` | `repo/API_tests/dashboard_date_filter.sh:2228, repo/API_tests/dashboard_date_filter.sh:2249, repo/API_tests/dashboard_date_filter.sh:2255, repo/API_tests/dashboard_date_filter.sh:2261, repo/API_tests/dashboard_unauthorized_returns_403_or_401.sh:2448` |
| `GET /api/v1/dashboards/drop-rate?<from>&<to>&<department_id>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:3219` |
| `GET /api/v1/dashboards/dwell-time?<from>&<to>&<department_id>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:3229` |
| `GET /api/v1/dashboards/fill-rate?<from>&<to>&<department_id>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:3209` |
| `GET /api/v1/dashboards/foot-traffic?<from>&<to>&<department_id>` | yes | true no-mock HTTP | `repo/API_tests/dashboard_derived_from_real_data.sh` | `repo/API_tests/dashboard_derived_from_real_data.sh:2336, repo/API_tests/dashboard_derived_from_real_data.sh:2354` |
| `GET /api/v1/dashboards/instructor-workload?<from>&<to>&<department_id>` | yes | true no-mock HTTP | `repo/API_tests/dashboard_masking_instructor_workload.sh` | `repo/API_tests/dashboard_masking_instructor_workload.sh:2415, repo/API_tests/dashboard_masking_instructor_workload.sh:2426` |
| `GET /api/v1/dashboards/interaction-quality?<from>&<to>&<department_id>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:3239` |
| `GET /api/v1/health` | yes | true no-mock HTTP | `repo/API_tests/health_check.sh` | `repo/API_tests/health_check.sh:2545` |
| `GET /api/v1/journals/<id>/versions/<version_id>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2679` |
| `GET /api/v1/journals/<id>/versions` | yes | true no-mock HTTP | `repo/API_tests/library_journal_happy_path.sh, repo/API_tests/library_publish_baseline_invariant.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/library_journal_happy_path.sh:2830, repo/API_tests/library_publish_baseline_invariant.sh:3077, repo/backend/tests/api_routes_test.rs:2664` |
| `GET /api/v1/journals/<id>` | yes | true no-mock HTTP | `repo/API_tests/library_journal_happy_path.sh, repo/API_tests/library_journal_not_found.sh` | `repo/API_tests/library_journal_happy_path.sh:2778, repo/API_tests/library_journal_not_found.sh:2857` |
| `GET /api/v1/journals/?<limit>&<offset>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2636` |
| `GET /api/v1/metrics/<id>/versions` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2756` |
| `GET /api/v1/metrics/<id>` | yes | true no-mock HTTP | `repo/API_tests/metric_crud_happy_path.sh` | `repo/API_tests/metric_crud_happy_path.sh:3227` |
| `GET /api/v1/metrics/?<limit>&<offset>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2705, repo/backend/tests/api_routes_test.rs:2740` |
| `GET /api/v1/reports/<id>/runs` | yes | true no-mock HTTP | `repo/API_tests/report_create_and_run.sh, repo/API_tests/report_scope_isolation.sh` | `repo/API_tests/report_create_and_run.sh:3715, repo/API_tests/report_scope_isolation.sh:4190` |
| `GET /api/v1/reports/<id>/schedules` | yes | true no-mock HTTP | `repo/API_tests/report_schedule_lifecycle.sh, repo/API_tests/report_scope_isolation.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/report_schedule_lifecycle.sh:4053, repo/API_tests/report_scope_isolation.sh:4242, repo/API_tests/report_scope_isolation.sh:4251, repo/API_tests/report_scope_isolation.sh:4257, repo/backend/tests/api_routes_test.rs:316, repo/backend/tests/api_routes_test.rs:333` |
| `GET /api/v1/reports/<id>` | yes | true no-mock HTTP | `repo/API_tests/report_create_and_run.sh, repo/API_tests/report_scope_isolation.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/report_create_and_run.sh:3687, repo/API_tests/report_scope_isolation.sh:4144, repo/API_tests/report_scope_isolation.sh:4150, repo/API_tests/report_scope_isolation.sh:4209, repo/backend/tests/api_routes_test.rs:811` |
| `GET /api/v1/reports/` | yes | true no-mock HTTP | `repo/API_tests/report_create_and_run.sh` | `repo/API_tests/report_create_and_run.sh:3679` |
| `GET /api/v1/reports/runs/<run_id>/download` | yes | true no-mock HTTP | `repo/API_tests/report_catalog_scope.sh, repo/API_tests/report_create_and_run.sh, repo/API_tests/report_crypto_erase.sh, repo/API_tests/report_department_scope.sh, repo/API_tests/report_scope_isolation.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/report_catalog_scope.sh:3521, repo/API_tests/report_catalog_scope.sh:3539, repo/API_tests/report_catalog_scope.sh:3604, repo/API_tests/report_catalog_scope.sh:3626, repo/API_tests/report_create_and_run.sh:3746, repo/API_tests/report_create_and_run.sh:3756, repo/API_tests/report_crypto_erase.sh:3826, repo/API_tests/report_crypto_erase.sh:3867, repo/API_tests/report_department_scope.sh:3944, repo/API_tests/report_department_scope.sh:3987, repo/API_tests/report_scope_isolation.sh:4196, repo/backend/tests/api_routes_test.rs:913, repo/backend/tests/api_routes_test.rs:1951, repo/backend/tests/api_routes_test.rs:1990, repo/backend/tests/api_routes_test.rs:2143, repo/backend/tests/api_routes_test.rs:2192` |
| `GET /api/v1/reports/runs/<run_id>` | yes | true no-mock HTTP | `repo/API_tests/report_create_and_run.sh, repo/API_tests/report_department_scope.sh, repo/API_tests/report_scope_isolation.sh` | `repo/API_tests/report_create_and_run.sh:3725, repo/API_tests/report_department_scope.sh:3935, repo/API_tests/report_department_scope.sh:3973, repo/API_tests/report_scope_isolation.sh:4168, repo/API_tests/report_scope_isolation.sh:4178, repo/API_tests/report_scope_isolation.sh:4184` |
| `GET /api/v1/roles/<id>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2610` |
| `GET /api/v1/roles/` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2594` |
| `GET /api/v1/sections/<id>/versions` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2915` |
| `GET /api/v1/sections/<id>` | yes | true no-mock HTTP | `repo/API_tests/academic_section_happy_path.sh, repo/API_tests/encryption_field_masking.sh` | `repo/API_tests/academic_section_happy_path.sh:727, repo/API_tests/encryption_field_masking.sh:2511` |
| `GET /api/v1/sections/?<course_id>&<department_id>&<limit>&<offset>` | yes | true no-mock HTTP | `repo/API_tests/academic_section_happy_path.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/academic_section_happy_path.sh:769, repo/backend/tests/api_routes_test.rs:1654, repo/backend/tests/api_routes_test.rs:2249, repo/backend/tests/api_routes_test.rs:2906` |
| `GET /api/v1/sections/export.csv` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2888` |
| `GET /api/v1/sections/export.xlsx` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2897` |
| `GET /api/v1/sections/template.csv` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2870` |
| `GET /api/v1/sections/template.xlsx` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2879` |
| `GET /api/v1/teaching-resources/<id>/versions/<version_id>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:3078` |
| `GET /api/v1/teaching-resources/<id>/versions` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:3063` |
| `GET /api/v1/teaching-resources/<id>` | yes | true no-mock HTTP | `repo/API_tests/library_resource_happy_path.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/library_resource_happy_path.sh:3129, repo/backend/tests/api_routes_test.rs:1277, repo/backend/tests/api_routes_test.rs:1319, repo/backend/tests/api_routes_test.rs:1362, repo/backend/tests/api_routes_test.rs:1579` |
| `GET /api/v1/teaching-resources/?<limit>&<offset>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:1594` |
| `GET /api/v1/users/<id>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:3119` |
| `GET /api/v1/users/` | yes | true no-mock HTTP | `repo/API_tests/user_crud.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/user_crud.sh:4570, repo/API_tests/user_crud.sh:4576, repo/backend/tests/api_routes_test.rs:3104` |
| `GET /api/v1/users/me` | yes | true no-mock HTTP | `repo/API_tests/user_crud.sh, repo/API_tests/auth_unauthenticated_access.sh` | `repo/API_tests/user_crud.sh:4555, repo/API_tests/auth_unauthenticated_access.sh:8` |
| `POST /api/v1/admin/artifact-backfill/` | yes | true no-mock HTTP | `repo/API_tests/artifact_backfill.sh, repo/API_tests/artifact_backfill_strict.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/artifact_backfill.sh:1025, repo/API_tests/artifact_backfill.sh:1032, repo/API_tests/artifact_backfill.sh:1040, repo/API_tests/artifact_backfill.sh:1072, repo/API_tests/artifact_backfill.sh:1106, repo/API_tests/artifact_backfill_strict.sh:1221, repo/API_tests/artifact_backfill_strict.sh:1254, repo/backend/tests/api_routes_test.rs:2456, repo/backend/tests/api_routes_test.rs:2539` |
| `POST /api/v1/admin/retention/<id>/execute` | yes | true no-mock HTTP | `repo/API_tests/report_crypto_erase.sh, repo/API_tests/retention_execute.sh` | `repo/API_tests/report_crypto_erase.sh:3858, repo/API_tests/retention_execute.sh:4415` |
| `POST /api/v1/admin/retention/` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:651, repo/backend/tests/api_routes_test.rs:2815` |
| `POST /api/v1/admin/retention/execute` | yes | true no-mock HTTP | `repo/API_tests/artifact_backfill_strict.sh, repo/API_tests/retention_execute.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/artifact_backfill_strict.sh:1152, repo/API_tests/artifact_backfill_strict.sh:1159, repo/API_tests/artifact_backfill_strict.sh:1167, repo/API_tests/artifact_backfill_strict.sh:1186, repo/API_tests/artifact_backfill_strict.sh:1284, repo/API_tests/artifact_backfill_strict.sh:1305, repo/API_tests/retention_execute.sh:4376, repo/API_tests/retention_execute.sh:4384, repo/API_tests/retention_execute.sh:4437, repo/backend/tests/api_routes_test.rs:2442, repo/backend/tests/api_routes_test.rs:2486, repo/backend/tests/api_routes_test.rs:2525` |
| `POST /api/v1/attachments/` | yes | true no-mock HTTP | `repo/API_tests/library_attachment_upload_and_preview.sh, repo/API_tests/library_attachment_validation.sh, repo/API_tests/library_preview_unsupported_type.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/library_attachment_upload_and_preview.sh:2608, repo/API_tests/library_attachment_validation.sh:2702, repo/API_tests/library_attachment_validation.sh:2712, repo/API_tests/library_attachment_validation.sh:2720, repo/API_tests/library_attachment_validation.sh:2731, repo/API_tests/library_preview_unsupported_type.sh:2968, repo/backend/tests/api_routes_test.rs:1434` |
| `POST /api/v1/auth/login` | yes | true no-mock HTTP | `repo/API_tests/_common.sh, repo/API_tests/auth_lockout.sh, repo/API_tests/auth_login_bad_password.sh, repo/API_tests/auth_login_success.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/_common.sh:56, repo/API_tests/auth_lockout.sh:1588, repo/API_tests/auth_lockout.sh:1599, repo/API_tests/auth_login_bad_password.sh:1615, repo/API_tests/auth_login_success.sh:1628, repo/backend/tests/api_routes_test.rs:95` |
| `POST /api/v1/auth/logout` | yes | true no-mock HTTP | `repo/API_tests/logout_revokes_session.sh` | `repo/API_tests/logout_revokes_session.sh:3190` |
| `POST /api/v1/checkins/<id>/retry` | yes | true no-mock HTTP | `repo/API_tests/checkin_network_blocked_retry.sh, repo/API_tests/checkin_retry_happy_path.sh, repo/API_tests/checkin_retry_invalid_reason.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/checkin_network_blocked_retry.sh:1965, repo/API_tests/checkin_network_blocked_retry.sh:1993, repo/API_tests/checkin_network_blocked_retry.sh:2004, repo/API_tests/checkin_retry_happy_path.sh:2071, repo/API_tests/checkin_retry_happy_path.sh:2087, repo/API_tests/checkin_retry_invalid_reason.sh:2142, repo/API_tests/checkin_retry_invalid_reason.sh:2150, repo/backend/tests/api_routes_test.rs:1798, repo/backend/tests/api_routes_test.rs:2353, repo/backend/tests/api_routes_test.rs:2383` |
| `POST /api/v1/checkins/` | yes | true no-mock HTTP | `repo/API_tests/checkin_duplicate_blocked.sh, repo/API_tests/checkin_happy_path.sh, repo/API_tests/checkin_masking.sh, repo/API_tests/checkin_network_blocked_retry.sh, repo/API_tests/checkin_retry_happy_path.sh, repo/API_tests/checkin_retry_invalid_reason.sh, repo/API_tests/checkin_unauthorized.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/checkin_duplicate_blocked.sh:1698, repo/API_tests/checkin_duplicate_blocked.sh:1706, repo/API_tests/checkin_happy_path.sh:1776, repo/API_tests/checkin_masking.sh:1836, repo/API_tests/checkin_network_blocked_retry.sh:1930, repo/API_tests/checkin_network_blocked_retry.sh:1946, repo/API_tests/checkin_retry_happy_path.sh:2053, repo/API_tests/checkin_retry_happy_path.sh:2063, repo/API_tests/checkin_retry_invalid_reason.sh:2132, repo/API_tests/checkin_unauthorized.sh:2194, repo/API_tests/checkin_unauthorized.sh:2203, repo/backend/tests/api_routes_test.rs:1696, repo/backend/tests/api_routes_test.rs:1738, repo/backend/tests/api_routes_test.rs:2279` |
| `POST /api/v1/courses/<id>/prerequisites` | yes | true no-mock HTTP | `repo/API_tests/academic_prerequisites.sh` | `repo/API_tests/academic_prerequisites.sh:631, repo/API_tests/academic_prerequisites.sh:652, repo/API_tests/academic_prerequisites.sh:661` |
| `POST /api/v1/courses/<id>/versions/<vid>/approve` | yes | true no-mock HTTP | `repo/API_tests/academic_course_happy_path.sh` | `repo/API_tests/academic_course_happy_path.sh:136` |
| `POST /api/v1/courses/<id>/versions/<vid>/publish` | yes | true no-mock HTTP | `repo/API_tests/academic_course_happy_path.sh` | `repo/API_tests/academic_course_happy_path.sh:146` |
| `POST /api/v1/courses/` | yes | true no-mock HTTP | `repo/API_tests/academic_course_happy_path.sh, repo/API_tests/academic_course_unauthorized.sh, repo/API_tests/academic_course_validation.sh, repo/API_tests/academic_export_scope.sh, repo/API_tests/academic_prerequisites.sh, repo/API_tests/academic_section_happy_path.sh, repo/API_tests/academic_section_validation.sh, repo/API_tests/checkin_duplicate_blocked.sh, repo/API_tests/checkin_happy_path.sh, repo/API_tests/checkin_masking.sh, repo/API_tests/checkin_network_blocked_retry.sh, repo/API_tests/checkin_retry_happy_path.sh, repo/API_tests/checkin_retry_invalid_reason.sh, repo/API_tests/dashboard_derived_from_real_data.sh, repo/API_tests/dashboard_masking_instructor_workload.sh` | `repo/API_tests/academic_course_happy_path.sh:95, repo/API_tests/academic_course_unauthorized.sh:187, repo/API_tests/academic_course_unauthorized.sh:195, repo/API_tests/academic_course_validation.sh:218, repo/API_tests/academic_course_validation.sh:227, repo/API_tests/academic_course_validation.sh:236, repo/API_tests/academic_course_validation.sh:248, repo/API_tests/academic_course_validation.sh:255, repo/API_tests/academic_export_scope.sh:288, repo/API_tests/academic_export_scope.sh:296, repo/API_tests/academic_prerequisites.sh:608, repo/API_tests/academic_prerequisites.sh:619, repo/API_tests/academic_section_happy_path.sh:705, repo/API_tests/academic_section_validation.sh:801, repo/API_tests/checkin_duplicate_blocked.sh:1680, repo/API_tests/checkin_happy_path.sh:1753, repo/API_tests/checkin_masking.sh:1819, repo/API_tests/checkin_network_blocked_retry.sh:1905, repo/API_tests/checkin_retry_happy_path.sh:2035, repo/API_tests/checkin_retry_invalid_reason.sh:2114, repo/API_tests/dashboard_derived_from_real_data.sh:2312, repo/API_tests/dashboard_masking_instructor_workload.sh:2400` |
| `POST /api/v1/courses/import?<mode>` | yes | true no-mock HTTP | `repo/API_tests/academic_import_commit.sh, repo/API_tests/academic_import_dry_run.sh, repo/API_tests/academic_import_unauthorized.sh` | `repo/API_tests/academic_import_commit.sh:366, repo/API_tests/academic_import_commit.sh:405, repo/API_tests/academic_import_dry_run.sh:451, repo/API_tests/academic_import_unauthorized.sh:569, repo/API_tests/academic_import_unauthorized.sh:577` |
| `POST /api/v1/journals/<id>/versions/<version_id>/approve` | yes | true no-mock HTTP | `repo/API_tests/library_journal_happy_path.sh, repo/API_tests/library_publish_baseline_invariant.sh` | `repo/API_tests/library_journal_happy_path.sh:2807, repo/API_tests/library_publish_baseline_invariant.sh:3038, repo/API_tests/library_publish_baseline_invariant.sh:3061` |
| `POST /api/v1/journals/<id>/versions/<version_id>/publish` | yes | true no-mock HTTP | `repo/API_tests/library_journal_happy_path.sh, repo/API_tests/library_publish_baseline_invariant.sh` | `repo/API_tests/library_journal_happy_path.sh:2817, repo/API_tests/library_publish_baseline_invariant.sh:3043, repo/API_tests/library_publish_baseline_invariant.sh:3066` |
| `POST /api/v1/journals/` | yes | true no-mock HTTP | `repo/API_tests/library_attachment_upload_and_preview.sh, repo/API_tests/library_attachment_validation.sh, repo/API_tests/library_journal_happy_path.sh, repo/API_tests/library_journal_unauthorized.sh, repo/API_tests/library_journal_validation.sh, repo/API_tests/library_preview_unsupported_type.sh, repo/API_tests/library_publish_baseline_invariant.sh, repo/API_tests/report_catalog_scope.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/library_attachment_upload_and_preview.sh:2592, repo/API_tests/library_attachment_validation.sh:2692, repo/API_tests/library_journal_happy_path.sh:2755, repo/API_tests/library_journal_unauthorized.sh:2882, repo/API_tests/library_journal_unauthorized.sh:2890, repo/API_tests/library_journal_validation.sh:2909, repo/API_tests/library_journal_validation.sh:2918, repo/API_tests/library_journal_validation.sh:2927, repo/API_tests/library_preview_unsupported_type.sh:2952, repo/API_tests/library_publish_baseline_invariant.sh:3022, repo/API_tests/report_catalog_scope.sh:3484, repo/backend/tests/api_routes_test.rs:1886, repo/backend/tests/api_routes_test.rs:2649` |
| `POST /api/v1/metrics/<id>/versions/<vid>/approve` | yes | true no-mock HTTP | `repo/API_tests/metric_crud_happy_path.sh, repo/API_tests/metric_publish_requires_admin.sh, repo/API_tests/metric_version_dependent_widget_flag.sh` | `repo/API_tests/metric_crud_happy_path.sh:3253, repo/API_tests/metric_publish_requires_admin.sh:3334, repo/API_tests/metric_version_dependent_widget_flag.sh:3394, repo/API_tests/metric_version_dependent_widget_flag.sh:3428` |
| `POST /api/v1/metrics/<id>/versions/<vid>/publish` | yes | true no-mock HTTP | `repo/API_tests/metric_crud_happy_path.sh, repo/API_tests/metric_publish_requires_admin.sh, repo/API_tests/metric_version_dependent_widget_flag.sh` | `repo/API_tests/metric_crud_happy_path.sh:3261, repo/API_tests/metric_publish_requires_admin.sh:3341, repo/API_tests/metric_version_dependent_widget_flag.sh:3399, repo/API_tests/metric_version_dependent_widget_flag.sh:3433` |
| `POST /api/v1/metrics/` | yes | true no-mock HTTP | `repo/API_tests/metric_crud_happy_path.sh, repo/API_tests/metric_lineage_validation.sh, repo/API_tests/metric_publish_requires_admin.sh, repo/API_tests/metric_version_dependent_widget_flag.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/metric_crud_happy_path.sh:3218, repo/API_tests/metric_lineage_validation.sh:3287, repo/API_tests/metric_publish_requires_admin.sh:3323, repo/API_tests/metric_version_dependent_widget_flag.sh:3386, repo/backend/tests/api_routes_test.rs:2725` |
| `POST /api/v1/metrics/widgets/<widget_id>/verify` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2780` |
| `POST /api/v1/reports/<id>/run` | yes | true no-mock HTTP | `repo/API_tests/report_catalog_scope.sh, repo/API_tests/report_create_and_run.sh, repo/API_tests/report_crypto_erase.sh, repo/API_tests/report_department_scope.sh, repo/API_tests/report_scope_isolation.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/report_catalog_scope.sh:3509, repo/API_tests/report_catalog_scope.sh:3532, repo/API_tests/report_catalog_scope.sh:3590, repo/API_tests/report_catalog_scope.sh:3619, repo/API_tests/report_create_and_run.sh:3700, repo/API_tests/report_crypto_erase.sh:3816, repo/API_tests/report_department_scope.sh:3923, repo/API_tests/report_department_scope.sh:3964, repo/API_tests/report_scope_isolation.sh:4157, repo/backend/tests/api_routes_test.rs:878, repo/backend/tests/api_routes_test.rs:1077, repo/backend/tests/api_routes_test.rs:1095, repo/backend/tests/api_routes_test.rs:1185, repo/backend/tests/api_routes_test.rs:1204, repo/backend/tests/api_routes_test.rs:1930, repo/backend/tests/api_routes_test.rs:1972, repo/backend/tests/api_routes_test.rs:2124, repo/backend/tests/api_routes_test.rs:2174` |
| `POST /api/v1/reports/<id>/schedules` | yes | true no-mock HTTP | `repo/API_tests/report_schedule_lifecycle.sh, repo/API_tests/report_scope_isolation.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/report_schedule_lifecycle.sh:4028, repo/API_tests/report_schedule_lifecycle.sh:4046, repo/API_tests/report_schedule_lifecycle.sh:4061, repo/API_tests/report_scope_isolation.sh:4222, repo/backend/tests/api_routes_test.rs:846` |
| `POST /api/v1/reports/` | yes | true no-mock HTTP | `repo/API_tests/report_catalog_scope.sh, repo/API_tests/report_create_and_run.sh, repo/API_tests/report_crypto_erase.sh, repo/API_tests/report_department_scope.sh, repo/API_tests/report_schedule_lifecycle.sh, repo/API_tests/report_scope_isolation.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/report_catalog_scope.sh:3495, repo/API_tests/report_catalog_scope.sh:3576, repo/API_tests/report_create_and_run.sh:3657, repo/API_tests/report_crypto_erase.sh:3801, repo/API_tests/report_department_scope.sh:3905, repo/API_tests/report_schedule_lifecycle.sh:4014, repo/API_tests/report_scope_isolation.sh:4128, repo/backend/tests/api_routes_test.rs:283, repo/backend/tests/api_routes_test.rs:784, repo/backend/tests/api_routes_test.rs:1032, repo/backend/tests/api_routes_test.rs:1050, repo/backend/tests/api_routes_test.rs:1155, repo/backend/tests/api_routes_test.rs:1905, repo/backend/tests/api_routes_test.rs:2099, repo/backend/tests/api_routes_test.rs:3154` |
| `POST /api/v1/sections/<id>/versions/<vid>/approve` | yes | true no-mock HTTP | `repo/API_tests/academic_section_happy_path.sh` | `repo/API_tests/academic_section_happy_path.sh:751` |
| `POST /api/v1/sections/<id>/versions/<vid>/publish` | yes | true no-mock HTTP | `repo/API_tests/academic_section_happy_path.sh` | `repo/API_tests/academic_section_happy_path.sh:758` |
| `POST /api/v1/sections/` | yes | true no-mock HTTP | `repo/API_tests/academic_section_happy_path.sh, repo/API_tests/academic_section_validation.sh, repo/API_tests/checkin_duplicate_blocked.sh, repo/API_tests/checkin_happy_path.sh, repo/API_tests/checkin_masking.sh, repo/API_tests/checkin_network_blocked_retry.sh, repo/API_tests/checkin_retry_happy_path.sh, repo/API_tests/checkin_retry_invalid_reason.sh, repo/API_tests/dashboard_derived_from_real_data.sh, repo/API_tests/dashboard_masking_instructor_workload.sh, repo/API_tests/encryption_field_masking.sh` | `repo/API_tests/academic_section_happy_path.sh:716, repo/API_tests/academic_section_validation.sh:812, repo/API_tests/academic_section_validation.sh:821, repo/API_tests/academic_section_validation.sh:830, repo/API_tests/academic_section_validation.sh:839, repo/API_tests/academic_section_validation.sh:846, repo/API_tests/checkin_duplicate_blocked.sh:1688, repo/API_tests/checkin_happy_path.sh:1763, repo/API_tests/checkin_masking.sh:1827, repo/API_tests/checkin_network_blocked_retry.sh:1913, repo/API_tests/checkin_retry_happy_path.sh:2043, repo/API_tests/checkin_retry_invalid_reason.sh:2122, repo/API_tests/dashboard_derived_from_real_data.sh:2320, repo/API_tests/dashboard_masking_instructor_workload.sh:2408, repo/API_tests/encryption_field_masking.sh:2492` |
| `POST /api/v1/sections/import?<mode>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:2935` |
| `POST /api/v1/teaching-resources/<id>/versions/<version_id>/approve` | yes | true no-mock HTTP | `repo/API_tests/library_resource_happy_path.sh` | `repo/API_tests/library_resource_happy_path.sh:3155` |
| `POST /api/v1/teaching-resources/<id>/versions/<version_id>/publish` | yes | true no-mock HTTP | `repo/API_tests/library_resource_happy_path.sh` | `repo/API_tests/library_resource_happy_path.sh:3163` |
| `POST /api/v1/teaching-resources/` | yes | true no-mock HTTP | `repo/API_tests/library_resource_happy_path.sh, repo/API_tests/report_catalog_scope.sh, repo/API_tests/resource_ownership_enforcement.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/library_resource_happy_path.sh:3113, repo/API_tests/report_catalog_scope.sh:3554, repo/API_tests/report_catalog_scope.sh:3565, repo/API_tests/resource_ownership_enforcement.sh:4294, repo/backend/tests/api_routes_test.rs:1260, repo/backend/tests/api_routes_test.rs:1299, repo/backend/tests/api_routes_test.rs:1411, repo/backend/tests/api_routes_test.rs:1539, repo/backend/tests/api_routes_test.rs:2050, repo/backend/tests/api_routes_test.rs:2074, repo/backend/tests/api_routes_test.rs:3048` |
| `POST /api/v1/users/` | yes | true no-mock HTTP | `repo/API_tests/auth_lockout.sh, repo/API_tests/user_crud.sh, repo/backend/tests/api_routes_test.rs` | `repo/API_tests/auth_lockout.sh:1579, repo/API_tests/user_crud.sh:4585, repo/API_tests/user_crud.sh:4609, repo/API_tests/user_crud.sh:4621, repo/API_tests/user_crud.sh:4664, repo/API_tests/user_crud.sh:4672, repo/backend/tests/api_routes_test.rs:535` |
| `PUT /api/v1/admin/config/<key>` | yes | true no-mock HTTP | `repo/API_tests/admin_config_write.sh` | `repo/API_tests/admin_config_write.sh:947` |
| `PUT /api/v1/admin/retention/<id>` | yes | true no-mock HTTP | `repo/API_tests/report_crypto_erase.sh, repo/API_tests/retention_policy_update.sh` | `repo/API_tests/report_crypto_erase.sh:3849, repo/API_tests/report_crypto_erase.sh:3880, repo/API_tests/retention_policy_update.sh:4501, repo/API_tests/retention_policy_update.sh:4511, repo/API_tests/retention_policy_update.sh:4519, repo/API_tests/retention_policy_update.sh:4527` |
| `PUT /api/v1/courses/<id>` | yes | true no-mock HTTP | `repo/API_tests/academic_course_happy_path.sh` | `repo/API_tests/academic_course_happy_path.sh:121` |
| `PUT /api/v1/journals/<id>` | yes | true no-mock HTTP | `repo/API_tests/library_journal_happy_path.sh, repo/API_tests/library_publish_baseline_invariant.sh` | `repo/API_tests/library_journal_happy_path.sh:2792, repo/API_tests/library_publish_baseline_invariant.sh:3031, repo/API_tests/library_publish_baseline_invariant.sh:3052` |
| `PUT /api/v1/metrics/<id>` | yes | true no-mock HTTP | `repo/API_tests/metric_crud_happy_path.sh, repo/API_tests/metric_lineage_validation.sh, repo/API_tests/metric_version_dependent_widget_flag.sh` | `repo/API_tests/metric_crud_happy_path.sh:3240, repo/API_tests/metric_lineage_validation.sh:3297, repo/API_tests/metric_version_dependent_widget_flag.sh:3421` |
| `PUT /api/v1/reports/<id>` | yes | true no-mock HTTP | `repo/backend/tests/api_routes_test.rs` | `repo/backend/tests/api_routes_test.rs:3178` |
| `PUT /api/v1/reports/schedules/<schedule_id>` | yes | true no-mock HTTP | `repo/API_tests/report_schedule_lifecycle.sh` | `repo/API_tests/report_schedule_lifecycle.sh:4069` |
| `PUT /api/v1/sections/<id>` | yes | true no-mock HTTP | `repo/API_tests/academic_section_happy_path.sh` | `repo/API_tests/academic_section_happy_path.sh:738` |
| `PUT /api/v1/teaching-resources/<id>` | yes | true no-mock HTTP | `repo/API_tests/library_resource_happy_path.sh, repo/API_tests/resource_ownership_enforcement.sh` | `repo/API_tests/library_resource_happy_path.sh:3140, repo/API_tests/resource_ownership_enforcement.sh:4312, repo/API_tests/resource_ownership_enforcement.sh:4323, repo/API_tests/resource_ownership_enforcement.sh:4334, repo/API_tests/resource_ownership_enforcement.sh:4346` |
| `PUT /api/v1/users/<id>` | yes | true no-mock HTTP | `repo/API_tests/user_crud.sh` | `repo/API_tests/user_crud.sh:4631` |

## API Test Classification
1. **True No-Mock HTTP**
- `repo/API_tests/*.sh` (curl-based live HTTP requests through `/api/v1/*`)
- `repo/backend/tests/api_routes_test.rs` (Rocket client hitting mounted routes from `build_rocket()`)

2. **HTTP with Mocking**
- **None found**.
- Mock scan evidence: no `jest.mock`, `vi.mock`, `sinon.stub`, or DI override-based HTTP test shortcuts found in `repo/API_tests`, `repo/backend/tests`.

3. **Non-HTTP (unit/integration without HTTP)**
- Backend inline unit tests in `repo/backend/src/**` (`#[cfg(test)]` modules).
- Frontend unit tests in `repo/frontend/src/tests/*.rs` and `repo/frontend/src/layouts/main_layout.rs`.

## Coverage Summary
- Total endpoints: **105**
- Endpoints with HTTP tests: **105**
- Endpoints with TRUE no-mock HTTP tests: **105**
- HTTP coverage: **100.00%**
- True API coverage: **100.00%**

## Unit Test Summary

### Backend Unit Tests
- Present: **Yes**
- Evidence files/modules (sample):
  - API/controller-level: `repo/backend/src/api/health.rs:96`
  - Services: `repo/backend/src/application/{authorization,course_service,section_service,report_service,retention_service,resource_service,checkin_service,dashboard_service,metric_service,journal_service,attachment_service,audit_service,import_service,artifact_backfill,artifact_crypto}.rs`
  - Core/auth/middleware-like logic: `repo/backend/src/application/{password,lockout,session,scope,principal,masking,encryption}.rs`
  - Domain/config/infra: `repo/backend/src/domain/{report,versioning}.rs`, `repo/backend/src/config/mod.rs`, `repo/backend/src/infrastructure/storage.rs`
- Important backend modules NOT unit-tested (direct inline-unit sense):
  - Most route modules besides health rely on HTTP/integration tests rather than inline unit tests (e.g., `repo/backend/src/api/{auth,users,roles,reports,retention,...}.rs`).
  - Repository modules under `repo/backend/src/infrastructure/repositories/*.rs` show no co-located `#[cfg(test)]` modules.

### Frontend Unit Tests (STRICT)
- Frontend test files detected:
  - `repo/frontend/src/tests/router_tests.rs`
  - `repo/frontend/src/tests/auth_state_tests.rs`
  - `repo/frontend/src/tests/role_tests.rs`
  - `repo/frontend/src/tests/api_error_tests.rs`
  - `repo/frontend/src/tests/mod.rs`
- Framework/tools detected:
  - Rust native unit test harness (`#[test]`, `cargo test`) in frontend crate.
- Components/modules covered (direct imports and assertions):
  - Router/route types + guard logic: `crate::router::AppRoute`, `crate::layouts::main_layout::nav_item_allowed`
  - Auth state behavior: `crate::state::AuthState`
  - Role model/parsing/ordering: `crate::types::Role`
  - API error model: `crate::api::client::ApiError`
- Important frontend components/modules NOT unit-tested yet:
  - Page components: `repo/frontend/src/pages/*.rs`
  - Reusable UI components: `repo/frontend/src/components/*.rs`
  - Hooks: `repo/frontend/src/hooks/{use_api.rs,use_auth.rs}`
  - API modules beyond `client` error shape: `repo/frontend/src/api/*.rs`

**Frontend unit tests: PRESENT**

### Cross-Layer Observation
- Backend API coverage is now exhaustive by static mapping.
- Frontend unit tests exist and cover core logic/state, but UI component/page-level testing depth is still lighter than backend API depth.

## API Observability Check
- Strong observability in most API tests:
  - Method/path explicit in curl commands.
  - Request bodies/params and response assertions present in many scripts and Rust integration tests.
- Remaining weak spots:
  - Some tests still mostly status-code assertions with limited response-body contract checks (e.g., auth protection checks).

## Tests Check
- `run_tests.sh` now includes a Docker-contained mode (`--docker`) and documents `docker-compose up -d` workflow (`repo/run_tests.sh:7-30`, `47-100`).
- Default path still supports local cargo execution (`repo/run_tests.sh:116+`).
- Classification against strict rubric:
  - Docker-based path: **OK**
  - Local dependency path still present: **FLAG (non-blocking, because Docker-contained option now exists)**

## End-to-End Expectations
- Fullstack FE↔BE browser-level E2E tests are not explicitly present as automated end-to-end suites.
- Current state has strong API tests + frontend unit tests, but no visible full browser journey automation.

## Test Coverage Score (0–100)
**90/100**

## Score Rationale
- + 100% endpoint HTTP coverage with true no-mock classification by static evidence.
- + No mocking/stubbing shortcuts detected in API tests.
- + Frontend unit tests are now clearly present with identifiable files.
- - UI-component/page-level frontend test depth remains moderate.
- - No explicit automated FE↔BE browser E2E suite evidence.
- - Some tests remain primarily status-code based.

## Key Gaps
- Frontend component/page interaction tests are still thin.
- No clear automated browser E2E suite for fullstack user flows.
- Local-cargo test path still exists in default runner path (though Docker mode is present).

## Confidence & Assumptions
- Confidence: **High** for endpoint inventory and static coverage mapping.
- Assumptions:
  - Coverage mapping used normalized parameterized paths (`<id>`, `${id}`, `{id}` treated equivalently).
  - Static evidence indicates intended route hits; runtime pass/fail is out of scope by constraint.

## Test Coverage Verdict
**PASS**

---

# README Audit

## README Location
- Required file exists: `repo/README.md`.

## Hard Gates

### Formatting
- PASS: structured markdown, readable headings/tables/code blocks.

### Startup Instructions
- PASS (fullstack): exact required command appears:
  - `docker-compose up` (`repo/README.md:542`)

### Access Method
- PASS: explicit URLs + ports provided:
  - Frontend `http://localhost:3000`
  - Backend `http://localhost:8000`
  - Evidence: `repo/README.md:556-559`

### Verification Method
- PASS: explicit API curl verification and UI flow steps provided:
  - `repo/README.md:572-607`

### Environment Rules (STRICT)
- PASS: no `npm install`, `pip install`, `apt-get`, runtime dependency-install instructions, or manual DB setup steps in README.
- Docker-first startup and validation are documented.

### Demo Credentials (auth conditional)
- PASS: auth exists and README provides email + password across all roles:
  - Admin, Librarian, Instructor, Department Head, Viewer, Auditor
  - Evidence: `repo/README.md:607-617`

## Engineering Quality
- Tech stack clarity: strong.
- Architecture explanation: strong.
- Testing instructions: detailed and explicit.
- Security/roles/workflows: clearly documented.
- Presentation quality: high, though long-form.

## High Priority Issues
- None.

## Medium Priority Issues
- README remains very long; critical run/verify instructions can be buried for first-time operators.

## Low Priority Issues
- Minor redundancy across phase-history sections.

## Hard Gate Failures
- None.

## README Verdict
**PASS**

## Combined Final Verdicts
- Test Coverage Audit: **PASS**
- README Audit: **PASS**
