// Code generated from Db2Parser.g4 by ANTLR 4.13.2. DO NOT EDIT.

package db2 // Db2Parser
import "github.com/antlr4-go/antlr/v4"

// Db2ParserListener is a complete listener for a parse tree produced by Db2Parser.
type Db2ParserListener interface {
	antlr.ParseTreeListener

	// EnterDb2_file is called when entering the db2_file production.
	EnterDb2_file(c *Db2_fileContext)

	// EnterBatch is called when entering the batch production.
	EnterBatch(c *BatchContext)

	// EnterSql_statement is called when entering the sql_statement production.
	EnterSql_statement(c *Sql_statementContext)

	// EnterSql_schema_statement is called when entering the sql_schema_statement production.
	EnterSql_schema_statement(c *Sql_schema_statementContext)

	// EnterSql_data_change_statement is called when entering the sql_data_change_statement production.
	EnterSql_data_change_statement(c *Sql_data_change_statementContext)

	// EnterSql_data_statement is called when entering the sql_data_statement production.
	EnterSql_data_statement(c *Sql_data_statementContext)

	// EnterSql_transaction_statement is called when entering the sql_transaction_statement production.
	EnterSql_transaction_statement(c *Sql_transaction_statementContext)

	// EnterSql_connection_statement is called when entering the sql_connection_statement production.
	EnterSql_connection_statement(c *Sql_connection_statementContext)

	// EnterSql_dynamic_statement is called when entering the sql_dynamic_statement production.
	EnterSql_dynamic_statement(c *Sql_dynamic_statementContext)

	// EnterSql_session_statement is called when entering the sql_session_statement production.
	EnterSql_session_statement(c *Sql_session_statementContext)

	// EnterSql_embedded_host_language_statement is called when entering the sql_embedded_host_language_statement production.
	EnterSql_embedded_host_language_statement(c *Sql_embedded_host_language_statementContext)

	// EnterSql_constrol_statement is called when entering the sql_constrol_statement production.
	EnterSql_constrol_statement(c *Sql_constrol_statementContext)

	// EnterSelect_statement is called when entering the select_statement production.
	EnterSelect_statement(c *Select_statementContext)

	// EnterRead_only_clause is called when entering the read_only_clause production.
	EnterRead_only_clause(c *Read_only_clauseContext)

	// EnterUpdate_clause is called when entering the update_clause production.
	EnterUpdate_clause(c *Update_clauseContext)

	// EnterOptimize_for_clause is called when entering the optimize_for_clause production.
	EnterOptimize_for_clause(c *Optimize_for_clauseContext)

	// EnterConcurrent_access_resolution_clause is called when entering the concurrent_access_resolution_clause production.
	EnterConcurrent_access_resolution_clause(c *Concurrent_access_resolution_clauseContext)

	// EnterDelete_statement is called when entering the delete_statement production.
	EnterDelete_statement(c *Delete_statementContext)

	// EnterDelete_statement_searched_delete is called when entering the delete_statement_searched_delete production.
	EnterDelete_statement_searched_delete(c *Delete_statement_searched_deleteContext)

	// EnterTable_or_view_name is called when entering the table_or_view_name production.
	EnterTable_or_view_name(c *Table_or_view_nameContext)

	// EnterDelete_statement_positioned_delete is called when entering the delete_statement_positioned_delete production.
	EnterDelete_statement_positioned_delete(c *Delete_statement_positioned_deleteContext)

	// EnterDelete_deltalake_statement is called when entering the delete_deltalake_statement production.
	EnterDelete_deltalake_statement(c *Delete_deltalake_statementContext)

	// EnterInsert_statement is called when entering the insert_statement production.
	EnterInsert_statement(c *Insert_statementContext)

	// EnterInsert_datalake_statement is called when entering the insert_datalake_statement production.
	EnterInsert_datalake_statement(c *Insert_datalake_statementContext)

	// EnterValues_item is called when entering the values_item production.
	EnterValues_item(c *Values_itemContext)

	// EnterMerge_statement is called when entering the merge_statement production.
	EnterMerge_statement(c *Merge_statementContext)

	// EnterTable_view_fullselect is called when entering the table_view_fullselect production.
	EnterTable_view_fullselect(c *Table_view_fullselectContext)

	// EnterCommon_table_expression_list is called when entering the common_table_expression_list production.
	EnterCommon_table_expression_list(c *Common_table_expression_listContext)

	// EnterMatching_condition is called when entering the matching_condition production.
	EnterMatching_condition(c *Matching_conditionContext)

	// EnterModification_operation is called when entering the modification_operation production.
	EnterModification_operation(c *Modification_operationContext)

	// EnterUpdate_operation is called when entering the update_operation production.
	EnterUpdate_operation(c *Update_operationContext)

	// EnterDelete_operation is called when entering the delete_operation production.
	EnterDelete_operation(c *Delete_operationContext)

	// EnterInsert_operation is called when entering the insert_operation production.
	EnterInsert_operation(c *Insert_operationContext)

	// EnterExpr_null_default_list is called when entering the expr_null_default_list production.
	EnterExpr_null_default_list(c *Expr_null_default_listContext)

	// EnterIsolation_level is called when entering the isolation_level production.
	EnterIsolation_level(c *Isolation_levelContext)

	// EnterTruncate_statement is called when entering the truncate_statement production.
	EnterTruncate_statement(c *Truncate_statementContext)

	// EnterUpdate_statement is called when entering the update_statement production.
	EnterUpdate_statement(c *Update_statementContext)

	// EnterUpdate_statement_searched_update is called when entering the update_statement_searched_update production.
	EnterUpdate_statement_searched_update(c *Update_statement_searched_updateContext)

	// EnterSkip_wait is called when entering the skip_wait production.
	EnterSkip_wait(c *Skip_waitContext)

	// EnterUpdate_statement_positioned_update is called when entering the update_statement_positioned_update production.
	EnterUpdate_statement_positioned_update(c *Update_statement_positioned_updateContext)

	// EnterInclude_columns is called when entering the include_columns production.
	EnterInclude_columns(c *Include_columnsContext)

	// EnterAssignment_clause is called when entering the assignment_clause production.
	EnterAssignment_clause(c *Assignment_clauseContext)

	// EnterAssignment_item is called when entering the assignment_item production.
	EnterAssignment_item(c *Assignment_itemContext)

	// EnterPeriod_clause is called when entering the period_clause production.
	EnterPeriod_clause(c *Period_clauseContext)

	// EnterTime_sec is called when entering the time_sec production.
	EnterTime_sec(c *Time_secContext)

	// EnterUpdate_datalake_statement is called when entering the update_datalake_statement production.
	EnterUpdate_datalake_statement(c *Update_datalake_statementContext)

	// EnterGrant_database_authorities_statement is called when entering the grant_database_authorities_statement production.
	EnterGrant_database_authorities_statement(c *Grant_database_authorities_statementContext)

	// EnterDb_privilege_list is called when entering the db_privilege_list production.
	EnterDb_privilege_list(c *Db_privilege_listContext)

	// EnterDb_privilege is called when entering the db_privilege production.
	EnterDb_privilege(c *Db_privilegeContext)

	// EnterGrantee is called when entering the grantee production.
	EnterGrantee(c *GranteeContext)

	// EnterGrantee_user_group is called when entering the grantee_user_group production.
	EnterGrantee_user_group(c *Grantee_user_groupContext)

	// EnterUser_group is called when entering the user_group production.
	EnterUser_group(c *User_groupContext)

	// EnterGrantee_list is called when entering the grantee_list production.
	EnterGrantee_list(c *Grantee_listContext)

	// EnterGrantee_list_public is called when entering the grantee_list_public production.
	EnterGrantee_list_public(c *Grantee_list_publicContext)

	// EnterGrantee_list_user_group is called when entering the grantee_list_user_group production.
	EnterGrantee_list_user_group(c *Grantee_list_user_groupContext)

	// EnterGrant_exemption_statement is called when entering the grant_exemption_statement production.
	EnterGrant_exemption_statement(c *Grant_exemption_statementContext)

	// EnterExemption_privilege is called when entering the exemption_privilege production.
	EnterExemption_privilege(c *Exemption_privilegeContext)

	// EnterGrant_global_variable_privileges_statement is called when entering the grant_global_variable_privileges_statement production.
	EnterGrant_global_variable_privileges_statement(c *Grant_global_variable_privileges_statementContext)

	// EnterVariable_privilege is called when entering the variable_privilege production.
	EnterVariable_privilege(c *Variable_privilegeContext)

	// EnterRead_write is called when entering the read_write production.
	EnterRead_write(c *Read_writeContext)

	// EnterWith_grant_option is called when entering the with_grant_option production.
	EnterWith_grant_option(c *With_grant_optionContext)

	// EnterGrant_index_privileges_statement is called when entering the grant_index_privileges_statement production.
	EnterGrant_index_privileges_statement(c *Grant_index_privileges_statementContext)

	// EnterGrant_module_privileges_statement is called when entering the grant_module_privileges_statement production.
	EnterGrant_module_privileges_statement(c *Grant_module_privileges_statementContext)

	// EnterGrant_package_privileges_statement is called when entering the grant_package_privileges_statement production.
	EnterGrant_package_privileges_statement(c *Grant_package_privileges_statementContext)

	// EnterPackage_privilege_list is called when entering the package_privilege_list production.
	EnterPackage_privilege_list(c *Package_privilege_listContext)

	// EnterPackage_privilege is called when entering the package_privilege production.
	EnterPackage_privilege(c *Package_privilegeContext)

	// EnterGrant_role_statement is called when entering the grant_role_statement production.
	EnterGrant_role_statement(c *Grant_role_statementContext)

	// EnterRole_list is called when entering the role_list production.
	EnterRole_list(c *Role_listContext)

	// EnterGrant_routine_privileges_statement is called when entering the grant_routine_privileges_statement production.
	EnterGrant_routine_privileges_statement(c *Grant_routine_privileges_statementContext)

	// EnterGrant_schema_privileges_statement is called when entering the grant_schema_privileges_statement production.
	EnterGrant_schema_privileges_statement(c *Grant_schema_privileges_statementContext)

	// EnterSchema_privilege_list is called when entering the schema_privilege_list production.
	EnterSchema_privilege_list(c *Schema_privilege_listContext)

	// EnterSchema_privilege is called when entering the schema_privilege production.
	EnterSchema_privilege(c *Schema_privilegeContext)

	// EnterGrant_security_label_statement is called when entering the grant_security_label_statement production.
	EnterGrant_security_label_statement(c *Grant_security_label_statementContext)

	// EnterGrant_sequence_privileges_statement is called when entering the grant_sequence_privileges_statement production.
	EnterGrant_sequence_privileges_statement(c *Grant_sequence_privileges_statementContext)

	// EnterSequence_privilege_list is called when entering the sequence_privilege_list production.
	EnterSequence_privilege_list(c *Sequence_privilege_listContext)

	// EnterSequence_privilege is called when entering the sequence_privilege production.
	EnterSequence_privilege(c *Sequence_privilegeContext)

	// EnterGrant_server_privileges_statement is called when entering the grant_server_privileges_statement production.
	EnterGrant_server_privileges_statement(c *Grant_server_privileges_statementContext)

	// EnterGrant_setsessionuser_privilege_statement is called when entering the grant_setsessionuser_privilege_statement production.
	EnterGrant_setsessionuser_privilege_statement(c *Grant_setsessionuser_privilege_statementContext)

	// EnterUser_list is called when entering the user_list production.
	EnterUser_list(c *User_listContext)

	// EnterUser_auth is called when entering the user_auth production.
	EnterUser_auth(c *User_authContext)

	// EnterGrant_table_space_privileges_statement is called when entering the grant_table_space_privileges_statement production.
	EnterGrant_table_space_privileges_statement(c *Grant_table_space_privileges_statementContext)

	// EnterGrant_table_view_or_nickname_privileges_statement is called when entering the grant_table_view_or_nickname_privileges_statement production.
	EnterGrant_table_view_or_nickname_privileges_statement(c *Grant_table_view_or_nickname_privileges_statementContext)

	// EnterTvn_privilege_list is called when entering the tvn_privilege_list production.
	EnterTvn_privilege_list(c *Tvn_privilege_listContext)

	// EnterTvn_privilege is called when entering the tvn_privilege production.
	EnterTvn_privilege(c *Tvn_privilegeContext)

	// EnterColumn_name_list_paren is called when entering the column_name_list_paren production.
	EnterColumn_name_list_paren(c *Column_name_list_parenContext)

	// EnterColumn_name_list is called when entering the column_name_list production.
	EnterColumn_name_list(c *Column_name_listContext)

	// EnterGrant_workload_privileges_statement is called when entering the grant_workload_privileges_statement production.
	EnterGrant_workload_privileges_statement(c *Grant_workload_privileges_statementContext)

	// EnterGrant_xsr_object_privileges_statement is called when entering the grant_xsr_object_privileges_statement production.
	EnterGrant_xsr_object_privileges_statement(c *Grant_xsr_object_privileges_statementContext)

	// EnterRevoke_database_authorities_statement is called when entering the revoke_database_authorities_statement production.
	EnterRevoke_database_authorities_statement(c *Revoke_database_authorities_statementContext)

	// EnterBy_all is called when entering the by_all production.
	EnterBy_all(c *By_allContext)

	// EnterRevoke_exemption_statement is called when entering the revoke_exemption_statement production.
	EnterRevoke_exemption_statement(c *Revoke_exemption_statementContext)

	// EnterRevoke_global_variable_privileges_statement is called when entering the revoke_global_variable_privileges_statement production.
	EnterRevoke_global_variable_privileges_statement(c *Revoke_global_variable_privileges_statementContext)

	// EnterRevoke_index_privileges_statement is called when entering the revoke_index_privileges_statement production.
	EnterRevoke_index_privileges_statement(c *Revoke_index_privileges_statementContext)

	// EnterRevoke_module_privileges_statement is called when entering the revoke_module_privileges_statement production.
	EnterRevoke_module_privileges_statement(c *Revoke_module_privileges_statementContext)

	// EnterRevoke_package_privileges_statement is called when entering the revoke_package_privileges_statement production.
	EnterRevoke_package_privileges_statement(c *Revoke_package_privileges_statementContext)

	// EnterRevoke_role_statement is called when entering the revoke_role_statement production.
	EnterRevoke_role_statement(c *Revoke_role_statementContext)

	// EnterRevoke_routine_privileges_statement is called when entering the revoke_routine_privileges_statement production.
	EnterRevoke_routine_privileges_statement(c *Revoke_routine_privileges_statementContext)

	// EnterRevoke_schema_privileges_statement is called when entering the revoke_schema_privileges_statement production.
	EnterRevoke_schema_privileges_statement(c *Revoke_schema_privileges_statementContext)

	// EnterRevoke_security_label_statement is called when entering the revoke_security_label_statement production.
	EnterRevoke_security_label_statement(c *Revoke_security_label_statementContext)

	// EnterRevoke_sequence_privileges_statement is called when entering the revoke_sequence_privileges_statement production.
	EnterRevoke_sequence_privileges_statement(c *Revoke_sequence_privileges_statementContext)

	// EnterRevoke_server_privileges_statement is called when entering the revoke_server_privileges_statement production.
	EnterRevoke_server_privileges_statement(c *Revoke_server_privileges_statementContext)

	// EnterRevoke_setsessionuser_privilege_statement is called when entering the revoke_setsessionuser_privilege_statement production.
	EnterRevoke_setsessionuser_privilege_statement(c *Revoke_setsessionuser_privilege_statementContext)

	// EnterRevoke_table_space_privileges_statement is called when entering the revoke_table_space_privileges_statement production.
	EnterRevoke_table_space_privileges_statement(c *Revoke_table_space_privileges_statementContext)

	// EnterRevoke_table_view_or_nickname_privileges_statement is called when entering the revoke_table_view_or_nickname_privileges_statement production.
	EnterRevoke_table_view_or_nickname_privileges_statement(c *Revoke_table_view_or_nickname_privileges_statementContext)

	// EnterRevoke_workload_privileges_statement is called when entering the revoke_workload_privileges_statement production.
	EnterRevoke_workload_privileges_statement(c *Revoke_workload_privileges_statementContext)

	// EnterRevoke_xsr_object_privileges_statement is called when entering the revoke_xsr_object_privileges_statement production.
	EnterRevoke_xsr_object_privileges_statement(c *Revoke_xsr_object_privileges_statementContext)

	// EnterUser_group_role is called when entering the user_group_role production.
	EnterUser_group_role(c *User_group_roleContext)

	// EnterRollback_statement is called when entering the rollback_statement production.
	EnterRollback_statement(c *Rollback_statementContext)

	// EnterSavepoint_statement is called when entering the savepoint_statement production.
	EnterSavepoint_statement(c *Savepoint_statementContext)

	// EnterRelease_savepoint_statement is called when entering the release_savepoint_statement production.
	EnterRelease_savepoint_statement(c *Release_savepoint_statementContext)

	// EnterAllocate_cursor_statement is called when entering the allocate_cursor_statement production.
	EnterAllocate_cursor_statement(c *Allocate_cursor_statementContext)

	// EnterAlter_audit_policy_statement is called when entering the alter_audit_policy_statement production.
	EnterAlter_audit_policy_statement(c *Alter_audit_policy_statementContext)

	// EnterStatus_spec is called when entering the status_spec production.
	EnterStatus_spec(c *Status_specContext)

	// EnterNormal_audit is called when entering the normal_audit production.
	EnterNormal_audit(c *Normal_auditContext)

	// EnterAlter_bufferpool_statement is called when entering the alter_bufferpool_statement production.
	EnterAlter_bufferpool_statement(c *Alter_bufferpool_statementContext)

	// EnterImmediate_deferred is called when entering the immediate_deferred production.
	EnterImmediate_deferred(c *Immediate_deferredContext)

	// EnterAlter_database_partition_group_statement is called when entering the alter_database_partition_group_statement production.
	EnterAlter_database_partition_group_statement(c *Alter_database_partition_group_statementContext)

	// EnterDb_partition_group_list_item is called when entering the db_partition_group_list_item production.
	EnterDb_partition_group_list_item(c *Db_partition_group_list_itemContext)

	// EnterDb_partition_num_nums is called when entering the db_partition_num_nums production.
	EnterDb_partition_num_nums(c *Db_partition_num_numsContext)

	// EnterDb_partitions_clause is called when entering the db_partitions_clause production.
	EnterDb_partitions_clause(c *Db_partitions_clauseContext)

	// EnterDb_partition_options is called when entering the db_partition_options production.
	EnterDb_partition_options(c *Db_partition_optionsContext)

	// EnterAlter_database_statement is called when entering the alter_database_statement production.
	EnterAlter_database_statement(c *Alter_database_statementContext)

	// EnterAlter_database_opts is called when entering the alter_database_opts production.
	EnterAlter_database_opts(c *Alter_database_optsContext)

	// EnterAlter_event_monitor_statement is called when entering the alter_event_monitor_statement production.
	EnterAlter_event_monitor_statement(c *Alter_event_monitor_statementContext)

	// EnterAlter_event_monitor_opts is called when entering the alter_event_monitor_opts production.
	EnterAlter_event_monitor_opts(c *Alter_event_monitor_optsContext)

	// EnterAlter_function_statement is called when entering the alter_function_statement production.
	EnterAlter_function_statement(c *Alter_function_statementContext)

	// EnterAlter_function_opts is called when entering the alter_function_opts production.
	EnterAlter_function_opts(c *Alter_function_optsContext)

	// EnterFunction_designator is called when entering the function_designator production.
	EnterFunction_designator(c *Function_designatorContext)

	// EnterData_type_list is called when entering the data_type_list production.
	EnterData_type_list(c *Data_type_listContext)

	// EnterData_type_list_paren is called when entering the data_type_list_paren production.
	EnterData_type_list_paren(c *Data_type_list_parenContext)

	// EnterAlter_histogram_template_statement is called when entering the alter_histogram_template_statement production.
	EnterAlter_histogram_template_statement(c *Alter_histogram_template_statementContext)

	// EnterAlter_index_statement is called when entering the alter_index_statement production.
	EnterAlter_index_statement(c *Alter_index_statementContext)

	// EnterYes_no is called when entering the yes_no production.
	EnterYes_no(c *Yes_noContext)

	// EnterAlter_mask_statement is called when entering the alter_mask_statement production.
	EnterAlter_mask_statement(c *Alter_mask_statementContext)

	// EnterEnable_disable is called when entering the enable_disable production.
	EnterEnable_disable(c *Enable_disableContext)

	// EnterAlter_method_statement is called when entering the alter_method_statement production.
	EnterAlter_method_statement(c *Alter_method_statementContext)

	// EnterMethod_designator is called when entering the method_designator production.
	EnterMethod_designator(c *Method_designatorContext)

	// EnterAlter_model_statement is called when entering the alter_model_statement production.
	EnterAlter_model_statement(c *Alter_model_statementContext)

	// EnterAlter_module_statement is called when entering the alter_module_statement production.
	EnterAlter_module_statement(c *Alter_module_statementContext)

	// EnterAlter_module_opts is called when entering the alter_module_opts production.
	EnterAlter_module_opts(c *Alter_module_optsContext)

	// EnterModule_function_definition is called when entering the module_function_definition production.
	EnterModule_function_definition(c *Module_function_definitionContext)

	// EnterModule_procedure_definition is called when entering the module_procedure_definition production.
	EnterModule_procedure_definition(c *Module_procedure_definitionContext)

	// EnterModule_type_definition is called when entering the module_type_definition production.
	EnterModule_type_definition(c *Module_type_definitionContext)

	// EnterModule_variable_definition is called when entering the module_variable_definition production.
	EnterModule_variable_definition(c *Module_variable_definitionContext)

	// EnterModule_condition_definition is called when entering the module_condition_definition production.
	EnterModule_condition_definition(c *Module_condition_definitionContext)

	// EnterModule_object_identification is called when entering the module_object_identification production.
	EnterModule_object_identification(c *Module_object_identificationContext)

	// EnterModule_function_designator is called when entering the module_function_designator production.
	EnterModule_function_designator(c *Module_function_designatorContext)

	// EnterModule_procedure_designator is called when entering the module_procedure_designator production.
	EnterModule_procedure_designator(c *Module_procedure_designatorContext)

	// EnterAlter_nickname_statement is called when entering the alter_nickname_statement production.
	EnterAlter_nickname_statement(c *Alter_nickname_statementContext)

	// EnterAlter_nickname_opts_1 is called when entering the alter_nickname_opts_1 production.
	EnterAlter_nickname_opts_1(c *Alter_nickname_opts_1Context)

	// EnterAlter_nickname_opts_1_item is called when entering the alter_nickname_opts_1_item production.
	EnterAlter_nickname_opts_1_item(c *Alter_nickname_opts_1_itemContext)

	// EnterAlter_nickname_opts_2 is called when entering the alter_nickname_opts_2 production.
	EnterAlter_nickname_opts_2(c *Alter_nickname_opts_2Context)

	// EnterAlter_nickname_opts_2_item is called when entering the alter_nickname_opts_2_item production.
	EnterAlter_nickname_opts_2_item(c *Alter_nickname_opts_2_itemContext)

	// EnterConstraint_alteration is called when entering the constraint_alteration production.
	EnterConstraint_alteration(c *Constraint_alterationContext)

	// EnterAlter_package_statement is called when entering the alter_package_statement production.
	EnterAlter_package_statement(c *Alter_package_statementContext)

	// EnterAlter_package_opts is called when entering the alter_package_opts production.
	EnterAlter_package_opts(c *Alter_package_optsContext)

	// EnterAlter_permission_statement is called when entering the alter_permission_statement production.
	EnterAlter_permission_statement(c *Alter_permission_statementContext)

	// EnterAlter_procedure_external_statement is called when entering the alter_procedure_external_statement production.
	EnterAlter_procedure_external_statement(c *Alter_procedure_external_statementContext)

	// EnterAlter_procedure_external_opts is called when entering the alter_procedure_external_opts production.
	EnterAlter_procedure_external_opts(c *Alter_procedure_external_optsContext)

	// EnterProcedure_designator is called when entering the procedure_designator production.
	EnterProcedure_designator(c *Procedure_designatorContext)

	// EnterAlter_procedure_sourced_statement is called when entering the alter_procedure_sourced_statement production.
	EnterAlter_procedure_sourced_statement(c *Alter_procedure_sourced_statementContext)

	// EnterParameter_alteration is called when entering the parameter_alteration production.
	EnterParameter_alteration(c *Parameter_alterationContext)

	// EnterAlter_procedure_sql_statement is called when entering the alter_procedure_sql_statement production.
	EnterAlter_procedure_sql_statement(c *Alter_procedure_sql_statementContext)

	// EnterAlter_schema_statement is called when entering the alter_schema_statement production.
	EnterAlter_schema_statement(c *Alter_schema_statementContext)

	// EnterNone_changes is called when entering the none_changes production.
	EnterNone_changes(c *None_changesContext)

	// EnterAlter_security_label_component_statement is called when entering the alter_security_label_component_statement production.
	EnterAlter_security_label_component_statement(c *Alter_security_label_component_statementContext)

	// EnterAdd_element_clause is called when entering the add_element_clause production.
	EnterAdd_element_clause(c *Add_element_clauseContext)

	// EnterArray_element_clause is called when entering the array_element_clause production.
	EnterArray_element_clause(c *Array_element_clauseContext)

	// EnterTree_element_clause is called when entering the tree_element_clause production.
	EnterTree_element_clause(c *Tree_element_clauseContext)

	// EnterAlter_security_policy_statement is called when entering the alter_security_policy_statement production.
	EnterAlter_security_policy_statement(c *Alter_security_policy_statementContext)

	// EnterAlter_security_policy_opts is called when entering the alter_security_policy_opts production.
	EnterAlter_security_policy_opts(c *Alter_security_policy_optsContext)

	// EnterAlter_sequence_statement is called when entering the alter_sequence_statement production.
	EnterAlter_sequence_statement(c *Alter_sequence_statementContext)

	// EnterAlter_sequence_opts is called when entering the alter_sequence_opts production.
	EnterAlter_sequence_opts(c *Alter_sequence_optsContext)

	// EnterAlter_server_statement is called when entering the alter_server_statement production.
	EnterAlter_server_statement(c *Alter_server_statementContext)

	// EnterAlter_server_opts is called when entering the alter_server_opts production.
	EnterAlter_server_opts(c *Alter_server_optsContext)

	// EnterAlter_service_class_statement is called when entering the alter_service_class_statement production.
	EnterAlter_service_class_statement(c *Alter_service_class_statementContext)

	// EnterAlter_service_class_opts is called when entering the alter_service_class_opts production.
	EnterAlter_service_class_opts(c *Alter_service_class_optsContext)

	// EnterDefault_on_off is called when entering the default_on_off production.
	EnterDefault_on_off(c *Default_on_offContext)

	// EnterDefault_high_medium_low is called when entering the default_high_medium_low production.
	EnterDefault_high_medium_low(c *Default_high_medium_lowContext)

	// EnterAlter_stogroup_statement is called when entering the alter_stogroup_statement production.
	EnterAlter_stogroup_statement(c *Alter_stogroup_statementContext)

	// EnterAlter_stogroup_opts is called when entering the alter_stogroup_opts production.
	EnterAlter_stogroup_opts(c *Alter_stogroup_optsContext)

	// EnterAlter_table_statement is called when entering the alter_table_statement production.
	EnterAlter_table_statement(c *Alter_table_statementContext)

	// EnterAlter_table_opts is called when entering the alter_table_opts production.
	EnterAlter_table_opts(c *Alter_table_optsContext)

	// EnterNull_on_off is called when entering the null_on_off production.
	EnterNull_on_off(c *Null_on_offContext)

	// EnterCascade_restrict is called when entering the cascade_restrict production.
	EnterCascade_restrict(c *Cascade_restrictContext)

	// EnterMaterialized_query_definition is called when entering the materialized_query_definition production.
	EnterMaterialized_query_definition(c *Materialized_query_definitionContext)

	// EnterRefreshable_table_options is called when entering the refreshable_table_options production.
	EnterRefreshable_table_options(c *Refreshable_table_optionsContext)

	// EnterColumn_alteration is called when entering the column_alteration production.
	EnterColumn_alteration(c *Column_alterationContext)

	// EnterGeneration_alteration is called when entering the generation_alteration production.
	EnterGeneration_alteration(c *Generation_alterationContext)

	// EnterIdentity_alteration is called when entering the identity_alteration production.
	EnterIdentity_alteration(c *Identity_alterationContext)

	// EnterGeneration_attribute is called when entering the generation_attribute production.
	EnterGeneration_attribute(c *Generation_attributeContext)

	// EnterAs_identity_clause is called when entering the as_identity_clause production.
	EnterAs_identity_clause(c *As_identity_clauseContext)

	// EnterAs_identity_clause_opts is called when entering the as_identity_clause_opts production.
	EnterAs_identity_clause_opts(c *As_identity_clause_optsContext)

	// EnterPeriod_definition_alter is called when entering the period_definition_alter production.
	EnterPeriod_definition_alter(c *Period_definition_alterContext)

	// EnterAdd_partition is called when entering the add_partition production.
	EnterAdd_partition(c *Add_partitionContext)

	// EnterBoundary_spec_alter is called when entering the boundary_spec_alter production.
	EnterBoundary_spec_alter(c *Boundary_spec_alterContext)

	// EnterAttach_partition is called when entering the attach_partition production.
	EnterAttach_partition(c *Attach_partitionContext)

	// EnterActivate_deactivate is called when entering the activate_deactivate production.
	EnterActivate_deactivate(c *Activate_deactivateContext)

	// EnterAlter_tablespace_statement is called when entering the alter_tablespace_statement production.
	EnterAlter_tablespace_statement(c *Alter_tablespace_statementContext)

	// EnterAlter_tablespace_opts is called when entering the alter_tablespace_opts production.
	EnterAlter_tablespace_opts(c *Alter_tablespace_optsContext)

	// EnterAdd_clause is called when entering the add_clause production.
	EnterAdd_clause(c *Add_clauseContext)

	// EnterDb_container_clause is called when entering the db_container_clause production.
	EnterDb_container_clause(c *Db_container_clauseContext)

	// EnterDb_container_clause_opts is called when entering the db_container_clause_opts production.
	EnterDb_container_clause_opts(c *Db_container_clause_optsContext)

	// EnterDrop_container_clause is called when entering the drop_container_clause production.
	EnterDrop_container_clause(c *Drop_container_clauseContext)

	// EnterFile_device is called when entering the file_device production.
	EnterFile_device(c *File_deviceContext)

	// EnterAll_containers_clause is called when entering the all_containers_clause production.
	EnterAll_containers_clause(c *All_containers_clauseContext)

	// EnterSystem_container_clause is called when entering the system_container_clause production.
	EnterSystem_container_clause(c *System_container_clauseContext)

	// EnterStripeset is called when entering the stripeset production.
	EnterStripeset(c *StripesetContext)

	// EnterKm is called when entering the km production.
	EnterKm(c *KmContext)

	// EnterKmg_percent is called when entering the kmg_percent production.
	EnterKmg_percent(c *Kmg_percentContext)

	// EnterAlter_threshold_statement is called when entering the alter_threshold_statement production.
	EnterAlter_threshold_statement(c *Alter_threshold_statementContext)

	// EnterAlter_threshold_opts is called when entering the alter_threshold_opts production.
	EnterAlter_threshold_opts(c *Alter_threshold_optsContext)

	// EnterAlter_threshold_predicate is called when entering the alter_threshold_predicate production.
	EnterAlter_threshold_predicate(c *Alter_threshold_predicateContext)

	// EnterAlter_threshold_exceeded_actions is called when entering the alter_threshold_exceeded_actions production.
	EnterAlter_threshold_exceeded_actions(c *Alter_threshold_exceeded_actionsContext)

	// EnterDt_units is called when entering the dt_units production.
	EnterDt_units(c *Dt_unitsContext)

	// EnterDt_units_with_seconds is called when entering the dt_units_with_seconds production.
	EnterDt_units_with_seconds(c *Dt_units_with_secondsContext)

	// EnterAlter_trigger_statement is called when entering the alter_trigger_statement production.
	EnterAlter_trigger_statement(c *Alter_trigger_statementContext)

	// EnterAlter_trusted_context_statement is called when entering the alter_trusted_context_statement production.
	EnterAlter_trusted_context_statement(c *Alter_trusted_context_statementContext)

	// EnterAlter_trusted_context_opts is called when entering the alter_trusted_context_opts production.
	EnterAlter_trusted_context_opts(c *Alter_trusted_context_optsContext)

	// EnterAlter_trusted_context_opts_alter_opts is called when entering the alter_trusted_context_opts_alter_opts production.
	EnterAlter_trusted_context_opts_alter_opts(c *Alter_trusted_context_opts_alter_optsContext)

	// EnterAddr_clause_encryption_val is called when entering the addr_clause_encryption_val production.
	EnterAddr_clause_encryption_val(c *Addr_clause_encryption_valContext)

	// EnterAddress_clause is called when entering the address_clause production.
	EnterAddress_clause(c *Address_clauseContext)

	// EnterUser_clause is called when entering the user_clause production.
	EnterUser_clause(c *User_clauseContext)

	// EnterUse_for_opts is called when entering the use_for_opts production.
	EnterUse_for_opts(c *Use_for_optsContext)

	// EnterUse_for_opts_2 is called when entering the use_for_opts_2 production.
	EnterUse_for_opts_2(c *Use_for_opts_2Context)

	// EnterAlter_type_statement is called when entering the alter_type_statement production.
	EnterAlter_type_statement(c *Alter_type_statementContext)

	// EnterAlter_type_opts is called when entering the alter_type_opts production.
	EnterAlter_type_opts(c *Alter_type_optsContext)

	// EnterMethod_identifier is called when entering the method_identifier production.
	EnterMethod_identifier(c *Method_identifierContext)

	// EnterMethod_options is called when entering the method_options production.
	EnterMethod_options(c *Method_optionsContext)

	// EnterAlter_usage_list_statement is called when entering the alter_usage_list_statement production.
	EnterAlter_usage_list_statement(c *Alter_usage_list_statementContext)

	// EnterAlter_usage_list_opts_item is called when entering the alter_usage_list_opts_item production.
	EnterAlter_usage_list_opts_item(c *Alter_usage_list_opts_itemContext)

	// EnterAlter_user_mapping_statement is called when entering the alter_user_mapping_statement production.
	EnterAlter_user_mapping_statement(c *Alter_user_mapping_statementContext)

	// EnterAlter_user_mapping_opts_item is called when entering the alter_user_mapping_opts_item production.
	EnterAlter_user_mapping_opts_item(c *Alter_user_mapping_opts_itemContext)

	// EnterAdd_set is called when entering the add_set production.
	EnterAdd_set(c *Add_setContext)

	// EnterAlter_view_statement is called when entering the alter_view_statement production.
	EnterAlter_view_statement(c *Alter_view_statementContext)

	// EnterAlter_view_opts is called when entering the alter_view_opts production.
	EnterAlter_view_opts(c *Alter_view_optsContext)

	// EnterAlter_work_action_set_statement is called when entering the alter_work_action_set_statement production.
	EnterAlter_work_action_set_statement(c *Alter_work_action_set_statementContext)

	// EnterAlter_work_action_set_opts is called when entering the alter_work_action_set_opts production.
	EnterAlter_work_action_set_opts(c *Alter_work_action_set_optsContext)

	// EnterWork_action_alteration is called when entering the work_action_alteration production.
	EnterWork_action_alteration(c *Work_action_alterationContext)

	// EnterWork_action_alteration_opts is called when entering the work_action_alteration_opts production.
	EnterWork_action_alteration_opts(c *Work_action_alteration_optsContext)

	// EnterAlter_action_types_clause is called when entering the alter_action_types_clause production.
	EnterAlter_action_types_clause(c *Alter_action_types_clauseContext)

	// EnterThreshold_predicate_clause is called when entering the threshold_predicate_clause production.
	EnterThreshold_predicate_clause(c *Threshold_predicate_clauseContext)

	// EnterAlter_work_class_set_statement is called when entering the alter_work_class_set_statement production.
	EnterAlter_work_class_set_statement(c *Alter_work_class_set_statementContext)

	// EnterAlter_work_class_set_opts is called when entering the alter_work_class_set_opts production.
	EnterAlter_work_class_set_opts(c *Alter_work_class_set_optsContext)

	// EnterWork_class_alteration is called when entering the work_class_alteration production.
	EnterWork_class_alteration(c *Work_class_alterationContext)

	// EnterWork_class_alteration_opts is called when entering the work_class_alteration_opts production.
	EnterWork_class_alteration_opts(c *Work_class_alteration_optsContext)

	// EnterFor_from_to_alter_clause is called when entering the for_from_to_alter_clause production.
	EnterFor_from_to_alter_clause(c *For_from_to_alter_clauseContext)

	// EnterSchema_alter_clause is called when entering the schema_alter_clause production.
	EnterSchema_alter_clause(c *Schema_alter_clauseContext)

	// EnterData_tag_alter_clause is called when entering the data_tag_alter_clause production.
	EnterData_tag_alter_clause(c *Data_tag_alter_clauseContext)

	// EnterAlter_workload_statement is called when entering the alter_workload_statement production.
	EnterAlter_workload_statement(c *Alter_workload_statementContext)

	// EnterAlter_workload_opts_item is called when entering the alter_workload_opts_item production.
	EnterAlter_workload_opts_item(c *Alter_workload_opts_itemContext)

	// EnterPackage_executable is called when entering the package_executable production.
	EnterPackage_executable(c *Package_executableContext)

	// EnterBase_none is called when entering the base_none production.
	EnterBase_none(c *Base_noneContext)

	// EnterExtended_base_none is called when entering the extended_base_none production.
	EnterExtended_base_none(c *Extended_base_noneContext)

	// EnterAlter_collect_activity_data_clause is called when entering the alter_collect_activity_data_clause production.
	EnterAlter_collect_activity_data_clause(c *Alter_collect_activity_data_clauseContext)

	// EnterWith_opts is called when entering the with_opts production.
	EnterWith_opts(c *With_optsContext)

	// EnterAlter_collect_history_clause is called when entering the alter_collect_history_clause production.
	EnterAlter_collect_history_clause(c *Alter_collect_history_clauseContext)

	// EnterAlter_collect_lock_wait_data_clause is called when entering the alter_collect_lock_wait_data_clause production.
	EnterAlter_collect_lock_wait_data_clause(c *Alter_collect_lock_wait_data_clauseContext)

	// EnterAlter_wrapper_statement is called when entering the alter_wrapper_statement production.
	EnterAlter_wrapper_statement(c *Alter_wrapper_statementContext)

	// EnterAlter_wrapper_opts_item is called when entering the alter_wrapper_opts_item production.
	EnterAlter_wrapper_opts_item(c *Alter_wrapper_opts_itemContext)

	// EnterAlter_xsrobject_statement is called when entering the alter_xsrobject_statement production.
	EnterAlter_xsrobject_statement(c *Alter_xsrobject_statementContext)

	// EnterString is called when entering the string production.
	EnterString(c *StringContext)

	// EnterString_constant is called when entering the string_constant production.
	EnterString_constant(c *String_constantContext)

	// EnterNumeric_constant is called when entering the numeric_constant production.
	EnterNumeric_constant(c *Numeric_constantContext)

	// EnterData_type is called when entering the data_type production.
	EnterData_type(c *Data_typeContext)

	// EnterAnchored_data_type is called when entering the anchored_data_type production.
	EnterAnchored_data_type(c *Anchored_data_typeContext)

	// EnterAnchored_non_row_data_type is called when entering the anchored_non_row_data_type production.
	EnterAnchored_non_row_data_type(c *Anchored_non_row_data_typeContext)

	// EnterAnchored_row_data_type is called when entering the anchored_row_data_type production.
	EnterAnchored_row_data_type(c *Anchored_row_data_typeContext)

	// EnterSource_data_type is called when entering the source_data_type production.
	EnterSource_data_type(c *Source_data_typeContext)

	// EnterData_type_constrainst is called when entering the data_type_constrainst production.
	EnterData_type_constrainst(c *Data_type_constrainstContext)

	// EnterCheck_condition is called when entering the check_condition production.
	EnterCheck_condition(c *Check_conditionContext)

	// EnterData_type_2 is called when entering the data_type_2 production.
	EnterData_type_2(c *Data_type_2Context)

	// EnterBuilt_in_type is called when entering the built_in_type production.
	EnterBuilt_in_type(c *Built_in_typeContext)

	// EnterInteger_paren is called when entering the integer_paren production.
	EnterInteger_paren(c *Integer_parenContext)

	// EnterInteger_kmg_paren is called when entering the integer_kmg_paren production.
	EnterInteger_kmg_paren(c *Integer_kmg_parenContext)

	// EnterChar_character is called when entering the char_character production.
	EnterChar_character(c *Char_characterContext)

	// EnterOctets_codeunits is called when entering the octets_codeunits production.
	EnterOctets_codeunits(c *Octets_codeunitsContext)

	// EnterCodeunits is called when entering the codeunits production.
	EnterCodeunits(c *CodeunitsContext)

	// EnterKmg is called when entering the kmg production.
	EnterKmg(c *KmgContext)

	// EnterRs_locator_variable is called when entering the rs_locator_variable production.
	EnterRs_locator_variable(c *Rs_locator_variableContext)

	// EnterInteger_constant_list is called when entering the integer_constant_list production.
	EnterInteger_constant_list(c *Integer_constant_listContext)

	// EnterInteger_constant is called when entering the integer_constant production.
	EnterInteger_constant(c *Integer_constantContext)

	// EnterInteger_value is called when entering the integer_value production.
	EnterInteger_value(c *Integer_valueContext)

	// EnterPositive_integer is called when entering the positive_integer production.
	EnterPositive_integer(c *Positive_integerContext)

	// EnterBigint_value is called when entering the bigint_value production.
	EnterBigint_value(c *Bigint_valueContext)

	// EnterBigint_constant is called when entering the bigint_constant production.
	EnterBigint_constant(c *Bigint_constantContext)

	// EnterMember_number is called when entering the member_number production.
	EnterMember_number(c *Member_numberContext)

	// EnterVersion_id is called when entering the version_id production.
	EnterVersion_id(c *Version_idContext)

	// EnterDrop_statement is called when entering the drop_statement production.
	EnterDrop_statement(c *Drop_statementContext)

	// EnterAlias_designator is called when entering the alias_designator production.
	EnterAlias_designator(c *Alias_designatorContext)

	// EnterService_class_designator is called when entering the service_class_designator production.
	EnterService_class_designator(c *Service_class_designatorContext)

	// EnterTablespace_name_list is called when entering the tablespace_name_list production.
	EnterTablespace_name_list(c *Tablespace_name_listContext)

	// EnterAssociate_locators_statement is called when entering the associate_locators_statement production.
	EnterAssociate_locators_statement(c *Associate_locators_statementContext)

	// EnterAudit_statement is called when entering the audit_statement production.
	EnterAudit_statement(c *Audit_statementContext)

	// EnterBegin_declare_section_statement is called when entering the begin_declare_section_statement production.
	EnterBegin_declare_section_statement(c *Begin_declare_section_statementContext)

	// EnterCall_statement is called when entering the call_statement production.
	EnterCall_statement(c *Call_statementContext)

	// EnterArg_list_paren is called when entering the arg_list_paren production.
	EnterArg_list_paren(c *Arg_list_parenContext)

	// EnterArg_list is called when entering the arg_list production.
	EnterArg_list(c *Arg_listContext)

	// EnterArgument is called when entering the argument production.
	EnterArgument(c *ArgumentContext)

	// EnterCase_statement is called when entering the case_statement production.
	EnterCase_statement(c *Case_statementContext)

	// EnterSearched_case_statement_when_clause is called when entering the searched_case_statement_when_clause production.
	EnterSearched_case_statement_when_clause(c *Searched_case_statement_when_clauseContext)

	// EnterSimple_case_statement_when_clause is called when entering the simple_case_statement_when_clause production.
	EnterSimple_case_statement_when_clause(c *Simple_case_statement_when_clauseContext)

	// EnterClose_statement is called when entering the close_statement production.
	EnterClose_statement(c *Close_statementContext)

	// EnterComment_statement is called when entering the comment_statement production.
	EnterComment_statement(c *Comment_statementContext)

	// EnterColumn_comment is called when entering the column_comment production.
	EnterColumn_comment(c *Column_commentContext)

	// EnterComment_objects is called when entering the comment_objects production.
	EnterComment_objects(c *Comment_objectsContext)

	// EnterCommit_statement is called when entering the commit_statement production.
	EnterCommit_statement(c *Commit_statementContext)

	// EnterConnect_type_1_statement is called when entering the connect_type_1_statement production.
	EnterConnect_type_1_statement(c *Connect_type_1_statementContext)

	// EnterAuthorization is called when entering the authorization production.
	EnterAuthorization(c *AuthorizationContext)

	// EnterPasswords is called when entering the passwords production.
	EnterPasswords(c *PasswordsContext)

	// EnterLock_block is called when entering the lock_block production.
	EnterLock_block(c *Lock_blockContext)

	// EnterAccesstoken is called when entering the accesstoken production.
	EnterAccesstoken(c *AccesstokenContext)

	// EnterToken is called when entering the token production.
	EnterToken(c *TokenContext)

	// EnterApi_key is called when entering the api_key production.
	EnterApi_key(c *Api_keyContext)

	// EnterToken_type is called when entering the token_type production.
	EnterToken_type(c *Token_typeContext)

	// EnterDeclare_cursor_statement is called when entering the declare_cursor_statement production.
	EnterDeclare_cursor_statement(c *Declare_cursor_statementContext)

	// EnterDeclare_global_temporary_table_statement is called when entering the declare_global_temporary_table_statement production.
	EnterDeclare_global_temporary_table_statement(c *Declare_global_temporary_table_statementContext)

	// EnterDescribe_statement is called when entering the describe_statement production.
	EnterDescribe_statement(c *Describe_statementContext)

	// EnterXquery_statement is called when entering the xquery_statement production.
	EnterXquery_statement(c *Xquery_statementContext)

	// EnterDescribe_input_statement is called when entering the describe_input_statement production.
	EnterDescribe_input_statement(c *Describe_input_statementContext)

	// EnterDescribe_output_statement is called when entering the describe_output_statement production.
	EnterDescribe_output_statement(c *Describe_output_statementContext)

	// EnterDisconnect_statement is called when entering the disconnect_statement production.
	EnterDisconnect_statement(c *Disconnect_statementContext)

	// EnterEnd_declare_section_statement is called when entering the end_declare_section_statement production.
	EnterEnd_declare_section_statement(c *End_declare_section_statementContext)

	// EnterExecute_statement is called when entering the execute_statement production.
	EnterExecute_statement(c *Execute_statementContext)

	// EnterHost_variable_expression is called when entering the host_variable_expression production.
	EnterHost_variable_expression(c *Host_variable_expressionContext)

	// EnterAssignment_target is called when entering the assignment_target production.
	EnterAssignment_target(c *Assignment_targetContext)

	// EnterExecute_immediate_statement is called when entering the execute_immediate_statement production.
	EnterExecute_immediate_statement(c *Execute_immediate_statementContext)

	// EnterExplain_statement is called when entering the explain_statement production.
	EnterExplain_statement(c *Explain_statementContext)

	// EnterExplainable_sql_statement is called when entering the explainable_sql_statement production.
	EnterExplainable_sql_statement(c *Explainable_sql_statementContext)

	// EnterFetch_statement is called when entering the fetch_statement production.
	EnterFetch_statement(c *Fetch_statementContext)

	// EnterFlush_bufferpools_statement is called when entering the flush_bufferpools_statement production.
	EnterFlush_bufferpools_statement(c *Flush_bufferpools_statementContext)

	// EnterFlush_event_monitor_statement is called when entering the flush_event_monitor_statement production.
	EnterFlush_event_monitor_statement(c *Flush_event_monitor_statementContext)

	// EnterFlush_federated_cache_statement is called when entering the flush_federated_cache_statement production.
	EnterFlush_federated_cache_statement(c *Flush_federated_cache_statementContext)

	// EnterFlush_optimization_profile_cache_statement is called when entering the flush_optimization_profile_cache_statement production.
	EnterFlush_optimization_profile_cache_statement(c *Flush_optimization_profile_cache_statementContext)

	// EnterFlush_package_cache_statement is called when entering the flush_package_cache_statement production.
	EnterFlush_package_cache_statement(c *Flush_package_cache_statementContext)

	// EnterFlush_authentication_cache_statement is called when entering the flush_authentication_cache_statement production.
	EnterFlush_authentication_cache_statement(c *Flush_authentication_cache_statementContext)

	// EnterFree_locator_statement is called when entering the free_locator_statement production.
	EnterFree_locator_statement(c *Free_locator_statementContext)

	// EnterGet_diagnostics_statement is called when entering the get_diagnostics_statement production.
	EnterGet_diagnostics_statement(c *Get_diagnostics_statementContext)

	// EnterStatement_information is called when entering the statement_information production.
	EnterStatement_information(c *Statement_informationContext)

	// EnterCondition_information is called when entering the condition_information production.
	EnterCondition_information(c *Condition_informationContext)

	// EnterCondition_var_assignment is called when entering the condition_var_assignment production.
	EnterCondition_var_assignment(c *Condition_var_assignmentContext)

	// EnterLock_table_statement is called when entering the lock_table_statement production.
	EnterLock_table_statement(c *Lock_table_statementContext)

	// EnterPipe_statement is called when entering the pipe_statement production.
	EnterPipe_statement(c *Pipe_statementContext)

	// EnterRefresh_table_statement is called when entering the refresh_table_statement production.
	EnterRefresh_table_statement(c *Refresh_table_statementContext)

	// EnterRelease_connection_statement is called when entering the release_connection_statement production.
	EnterRelease_connection_statement(c *Release_connection_statementContext)

	// EnterRename_statement is called when entering the rename_statement production.
	EnterRename_statement(c *Rename_statementContext)

	// EnterRename_stogroup_statement is called when entering the rename_stogroup_statement production.
	EnterRename_stogroup_statement(c *Rename_stogroup_statementContext)

	// EnterRename_tablespace_statement is called when entering the rename_tablespace_statement production.
	EnterRename_tablespace_statement(c *Rename_tablespace_statementContext)

	// EnterSet_statement is called when entering the set_statement production.
	EnterSet_statement(c *Set_statementContext)

	// EnterAccess_mode_clause is called when entering the access_mode_clause production.
	EnterAccess_mode_clause(c *Access_mode_clauseContext)

	// EnterCascade_clause is called when entering the cascade_clause production.
	EnterCascade_clause(c *Cascade_clauseContext)

	// EnterTo_descendent_types is called when entering the to_descendent_types production.
	EnterTo_descendent_types(c *To_descendent_typesContext)

	// EnterTable_type_list is called when entering the table_type_list production.
	EnterTable_type_list(c *Table_type_listContext)

	// EnterTable_type is called when entering the table_type production.
	EnterTable_type(c *Table_typeContext)

	// EnterTable_checked_options_list is called when entering the table_checked_options_list production.
	EnterTable_checked_options_list(c *Table_checked_options_listContext)

	// EnterTable_checked_options is called when entering the table_checked_options production.
	EnterTable_checked_options(c *Table_checked_optionsContext)

	// EnterOnline_options is called when entering the online_options production.
	EnterOnline_options(c *Online_optionsContext)

	// EnterQuery_optimization_options is called when entering the query_optimization_options production.
	EnterQuery_optimization_options(c *Query_optimization_optionsContext)

	// EnterCheck_options is called when entering the check_options production.
	EnterCheck_options(c *Check_optionsContext)

	// EnterIncremental_options is called when entering the incremental_options production.
	EnterIncremental_options(c *Incremental_optionsContext)

	// EnterException_clause is called when entering the exception_clause production.
	EnterException_clause(c *Exception_clauseContext)

	// EnterIn_table_use_clause is called when entering the in_table_use_clause production.
	EnterIn_table_use_clause(c *In_table_use_clauseContext)

	// EnterTable_unchecked_options is called when entering the table_unchecked_options production.
	EnterTable_unchecked_options(c *Table_unchecked_optionsContext)

	// EnterFull_access is called when entering the full_access production.
	EnterFull_access(c *Full_accessContext)

	// EnterIntegrity_options is called when entering the integrity_options production.
	EnterIntegrity_options(c *Integrity_optionsContext)

	// EnterIntegrity_options_item is called when entering the integrity_options_item production.
	EnterIntegrity_options_item(c *Integrity_options_itemContext)

	// EnterVar_def_list is called when entering the var_def_list production.
	EnterVar_def_list(c *Var_def_listContext)

	// EnterVar_def is called when entering the var_def production.
	EnterVar_def(c *Var_defContext)

	// EnterExpr_null is called when entering the expr_null production.
	EnterExpr_null(c *Expr_nullContext)

	// EnterExpr_null_default is called when entering the expr_null_default production.
	EnterExpr_null_default(c *Expr_null_defaultContext)

	// EnterArray_index is called when entering the array_index production.
	EnterArray_index(c *Array_indexContext)

	// EnterRow_fullselect is called when entering the row_fullselect production.
	EnterRow_fullselect(c *Row_fullselectContext)

	// EnterTarget_variable is called when entering the target_variable production.
	EnterTarget_variable(c *Target_variableContext)

	// EnterTarget_cursor_variable is called when entering the target_cursor_variable production.
	EnterTarget_cursor_variable(c *Target_cursor_variableContext)

	// EnterTarget_row_variable is called when entering the target_row_variable production.
	EnterTarget_row_variable(c *Target_row_variableContext)

	// EnterRow_array_element_specification is called when entering the row_array_element_specification production.
	EnterRow_array_element_specification(c *Row_array_element_specificationContext)

	// EnterRow_field_reference is called when entering the row_field_reference production.
	EnterRow_field_reference(c *Row_field_referenceContext)

	// EnterField_reference is called when entering the field_reference production.
	EnterField_reference(c *Field_referenceContext)

	// EnterSearch_condition is called when entering the search_condition production.
	EnterSearch_condition(c *Search_conditionContext)

	// EnterPredicate is called when entering the predicate production.
	EnterPredicate(c *PredicateContext)

	// EnterAccording_to_clause is called when entering the according_to_clause production.
	EnterAccording_to_clause(c *According_to_clauseContext)

	// EnterXml_schema_identification_list is called when entering the xml_schema_identification_list production.
	EnterXml_schema_identification_list(c *Xml_schema_identification_listContext)

	// EnterXml_schema_identification is called when entering the xml_schema_identification production.
	EnterXml_schema_identification(c *Xml_schema_identificationContext)

	// EnterFullselect_in_parentheses is called when entering the fullselect_in_parentheses production.
	EnterFullselect_in_parentheses(c *Fullselect_in_parenthesesContext)

	// EnterSome_any_all is called when entering the some_any_all production.
	EnterSome_any_all(c *Some_any_allContext)

	// EnterRow_value_expression is called when entering the row_value_expression production.
	EnterRow_value_expression(c *Row_value_expressionContext)

	// EnterComparison_operator is called when entering the comparison_operator production.
	EnterComparison_operator(c *Comparison_operatorContext)

	// EnterRow_expression is called when entering the row_expression production.
	EnterRow_expression(c *Row_expressionContext)

	// EnterPath_opt_list is called when entering the path_opt_list production.
	EnterPath_opt_list(c *Path_opt_listContext)

	// EnterPath_opt is called when entering the path_opt production.
	EnterPath_opt(c *Path_optContext)

	// EnterPkg_opt_list is called when entering the pkg_opt_list production.
	EnterPkg_opt_list(c *Pkg_opt_listContext)

	// EnterPkg_opt is called when entering the pkg_opt production.
	EnterPkg_opt(c *Pkg_optContext)

	// EnterMaintain_opt_list is called when entering the maintain_opt_list production.
	EnterMaintain_opt_list(c *Maintain_opt_listContext)

	// EnterMaintain_opt is called when entering the maintain_opt production.
	EnterMaintain_opt(c *Maintain_optContext)

	// EnterVariable is called when entering the variable production.
	EnterVariable(c *VariableContext)

	// EnterHost_variable is called when entering the host_variable production.
	EnterHost_variable(c *Host_variableContext)

	// EnterSet_integrity_statement is called when entering the set_integrity_statement production.
	EnterSet_integrity_statement(c *Set_integrity_statementContext)

	// EnterTransfer_ownership_statement is called when entering the transfer_ownership_statement production.
	EnterTransfer_ownership_statement(c *Transfer_ownership_statementContext)

	// EnterObjects is called when entering the objects production.
	EnterObjects(c *ObjectsContext)

	// EnterWhenever_statement is called when entering the whenever_statement production.
	EnterWhenever_statement(c *Whenever_statementContext)

	// EnterFor_statement is called when entering the for_statement production.
	EnterFor_statement(c *For_statementContext)

	// EnterGoto_statement is called when entering the goto_statement production.
	EnterGoto_statement(c *Goto_statementContext)

	// EnterIf_statement is called when entering the if_statement production.
	EnterIf_statement(c *If_statementContext)

	// EnterInclude_statement is called when entering the include_statement production.
	EnterInclude_statement(c *Include_statementContext)

	// EnterResignal_statement is called when entering the resignal_statement production.
	EnterResignal_statement(c *Resignal_statementContext)

	// EnterSignal_information is called when entering the signal_information production.
	EnterSignal_information(c *Signal_informationContext)

	// EnterDiagnostic_string_constant is called when entering the diagnostic_string_constant production.
	EnterDiagnostic_string_constant(c *Diagnostic_string_constantContext)

	// EnterSignal_statement is called when entering the signal_statement production.
	EnterSignal_statement(c *Signal_statementContext)

	// EnterSqlstate_string_constant is called when entering the sqlstate_string_constant production.
	EnterSqlstate_string_constant(c *Sqlstate_string_constantContext)

	// EnterSqlstate_string_variable is called when entering the sqlstate_string_variable production.
	EnterSqlstate_string_variable(c *Sqlstate_string_variableContext)

	// EnterSignal_information_2 is called when entering the signal_information_2 production.
	EnterSignal_information_2(c *Signal_information_2Context)

	// EnterDiagnostic_string_expression is called when entering the diagnostic_string_expression production.
	EnterDiagnostic_string_expression(c *Diagnostic_string_expressionContext)

	// EnterIterate_statement is called when entering the iterate_statement production.
	EnterIterate_statement(c *Iterate_statementContext)

	// EnterLeave_statement is called when entering the leave_statement production.
	EnterLeave_statement(c *Leave_statementContext)

	// EnterLoop_statement is called when entering the loop_statement production.
	EnterLoop_statement(c *Loop_statementContext)

	// EnterOpen_statement is called when entering the open_statement production.
	EnterOpen_statement(c *Open_statementContext)

	// EnterVariable_or_expression is called when entering the variable_or_expression production.
	EnterVariable_or_expression(c *Variable_or_expressionContext)

	// EnterSelect_into_statement is called when entering the select_into_statement production.
	EnterSelect_into_statement(c *Select_into_statementContext)

	// EnterValues_into_statement is called when entering the values_into_statement production.
	EnterValues_into_statement(c *Values_into_statementContext)

	// EnterPrepare_statement is called when entering the prepare_statement production.
	EnterPrepare_statement(c *Prepare_statementContext)

	// EnterRepeat_statement is called when entering the repeat_statement production.
	EnterRepeat_statement(c *Repeat_statementContext)

	// EnterReturn_statement is called when entering the return_statement production.
	EnterReturn_statement(c *Return_statementContext)

	// EnterWhile_statement is called when entering the while_statement production.
	EnterWhile_statement(c *While_statementContext)

	// EnterSql_routine_statement is called when entering the sql_routine_statement production.
	EnterSql_routine_statement(c *Sql_routine_statementContext)

	// EnterCommon_table_expression is called when entering the common_table_expression production.
	EnterCommon_table_expression(c *Common_table_expressionContext)

	// EnterCreate_alias_statement is called when entering the create_alias_statement production.
	EnterCreate_alias_statement(c *Create_alias_statementContext)

	// EnterTable_alias is called when entering the table_alias production.
	EnterTable_alias(c *Table_aliasContext)

	// EnterModule_alias is called when entering the module_alias production.
	EnterModule_alias(c *Module_aliasContext)

	// EnterSequence_alias is called when entering the sequence_alias production.
	EnterSequence_alias(c *Sequence_aliasContext)

	// EnterOr_replace is called when entering the or_replace production.
	EnterOr_replace(c *Or_replaceContext)

	// EnterCreate_audit_policy_statement is called when entering the create_audit_policy_statement production.
	EnterCreate_audit_policy_statement(c *Create_audit_policy_statementContext)

	// EnterAudit_policy_opts is called when entering the audit_policy_opts production.
	EnterAudit_policy_opts(c *Audit_policy_optsContext)

	// EnterAudit_policy_categories_opts is called when entering the audit_policy_categories_opts production.
	EnterAudit_policy_categories_opts(c *Audit_policy_categories_optsContext)

	// EnterCreate_bufferpool_statement is called when entering the create_bufferpool_statement production.
	EnterCreate_bufferpool_statement(c *Create_bufferpool_statementContext)

	// EnterBufferpool_opts is called when entering the bufferpool_opts production.
	EnterBufferpool_opts(c *Bufferpool_optsContext)

	// EnterExcept_clause is called when entering the except_clause production.
	EnterExcept_clause(c *Except_clauseContext)

	// EnterMember_list is called when entering the member_list production.
	EnterMember_list(c *Member_listContext)

	// EnterMember_list_item is called when entering the member_list_item production.
	EnterMember_list_item(c *Member_list_itemContext)

	// EnterCreate_database_partition_group_statement is called when entering the create_database_partition_group_statement production.
	EnterCreate_database_partition_group_statement(c *Create_database_partition_group_statementContext)

	// EnterCreate_event_monitor_statement is called when entering the create_event_monitor_statement production.
	EnterCreate_event_monitor_statement(c *Create_event_monitor_statementContext)

	// EnterCreate_event_monitor_activities_statement is called when entering the create_event_monitor_activities_statement production.
	EnterCreate_event_monitor_activities_statement(c *Create_event_monitor_activities_statementContext)

	// EnterFormatted_event_table_info_3 is called when entering the formatted_event_table_info_3 production.
	EnterFormatted_event_table_info_3(c *Formatted_event_table_info_3Context)

	// EnterCreate_event_monitor_change_history_statement is called when entering the create_event_monitor_change_history_statement production.
	EnterCreate_event_monitor_change_history_statement(c *Create_event_monitor_change_history_statementContext)

	// EnterEvent_control_list is called when entering the event_control_list production.
	EnterEvent_control_list(c *Event_control_listContext)

	// EnterEvent_control is called when entering the event_control production.
	EnterEvent_control(c *Event_controlContext)

	// EnterCreate_event_monitor_locking_statement is called when entering the create_event_monitor_locking_statement production.
	EnterCreate_event_monitor_locking_statement(c *Create_event_monitor_locking_statementContext)

	// EnterCreate_event_monitor_package_cache_statement is called when entering the create_event_monitor_package_cache_statement production.
	EnterCreate_event_monitor_package_cache_statement(c *Create_event_monitor_package_cache_statementContext)

	// EnterFilter_and_collection_options is called when entering the filter_and_collection_options production.
	EnterFilter_and_collection_options(c *Filter_and_collection_optionsContext)

	// EnterEvent_condition is called when entering the event_condition production.
	EnterEvent_condition(c *Event_conditionContext)

	// EnterEvent_condition_item is called when entering the event_condition_item production.
	EnterEvent_condition_item(c *Event_condition_itemContext)

	// EnterCreate_event_monitor_statistics_statement is called when entering the create_event_monitor_statistics_statement production.
	EnterCreate_event_monitor_statistics_statement(c *Create_event_monitor_statistics_statementContext)

	// EnterEvent_monitor_statistics_opts is called when entering the event_monitor_statistics_opts production.
	EnterEvent_monitor_statistics_opts(c *Event_monitor_statistics_optsContext)

	// EnterCreate_event_monitor_threshold_violations_statement is called when entering the create_event_monitor_threshold_violations_statement production.
	EnterCreate_event_monitor_threshold_violations_statement(c *Create_event_monitor_threshold_violations_statementContext)

	// EnterFormatted_event_table_info_2 is called when entering the formatted_event_table_info_2 production.
	EnterFormatted_event_table_info_2(c *Formatted_event_table_info_2Context)

	// EnterFile_options is called when entering the file_options production.
	EnterFile_options(c *File_optionsContext)

	// EnterEvent_monitor_threshold_opts is called when entering the event_monitor_threshold_opts production.
	EnterEvent_monitor_threshold_opts(c *Event_monitor_threshold_optsContext)

	// EnterPages is called when entering the pages production.
	EnterPages(c *PagesContext)

	// EnterCreate_event_monitor_unit_of_work is called when entering the create_event_monitor_unit_of_work production.
	EnterCreate_event_monitor_unit_of_work(c *Create_event_monitor_unit_of_workContext)

	// EnterFormatted_event_table_info is called when entering the formatted_event_table_info production.
	EnterFormatted_event_table_info(c *Formatted_event_table_infoContext)

	// EnterAutostart_manualstart is called when entering the autostart_manualstart production.
	EnterAutostart_manualstart(c *Autostart_manualstartContext)

	// EnterEvm_group is called when entering the evm_group production.
	EnterEvm_group(c *Evm_groupContext)

	// EnterTarget_table_options is called when entering the target_table_options production.
	EnterTarget_table_options(c *Target_table_optionsContext)

	// EnterCreate_external_table_statement is called when entering the create_external_table_statement production.
	EnterCreate_external_table_statement(c *Create_external_table_statementContext)

	// EnterExt_table_option is called when entering the ext_table_option production.
	EnterExt_table_option(c *Ext_table_optionContext)

	// EnterExt_table_option_value is called when entering the ext_table_option_value production.
	EnterExt_table_option_value(c *Ext_table_option_valueContext)

	// EnterCreate_function_statement is called when entering the create_function_statement production.
	EnterCreate_function_statement(c *Create_function_statementContext)

	// EnterCreate_function_aggregate_interface_statement is called when entering the create_function_aggregate_interface_statement production.
	EnterCreate_function_aggregate_interface_statement(c *Create_function_aggregate_interface_statementContext)

	// EnterAgg_fn_param_decl is called when entering the agg_fn_param_decl production.
	EnterAgg_fn_param_decl(c *Agg_fn_param_declContext)

	// EnterAgg_fn_option_list is called when entering the agg_fn_option_list production.
	EnterAgg_fn_option_list(c *Agg_fn_option_listContext)

	// EnterState_variable_declaration is called when entering the state_variable_declaration production.
	EnterState_variable_declaration(c *State_variable_declarationContext)

	// EnterCreate_function_external_scalar_statement is called when entering the create_function_external_scalar_statement production.
	EnterCreate_function_external_scalar_statement(c *Create_function_external_scalar_statementContext)

	// EnterExt_scalar_param_decl is called when entering the ext_scalar_param_decl production.
	EnterExt_scalar_param_decl(c *Ext_scalar_param_declContext)

	// EnterExt_scalar_option_list is called when entering the ext_scalar_option_list production.
	EnterExt_scalar_option_list(c *Ext_scalar_option_listContext)

	// EnterExt_scalar_option_list_item is called when entering the ext_scalar_option_list_item production.
	EnterExt_scalar_option_list_item(c *Ext_scalar_option_list_itemContext)

	// EnterPredicate_specification is called when entering the predicate_specification production.
	EnterPredicate_specification(c *Predicate_specificationContext)

	// EnterData_filter is called when entering the data_filter production.
	EnterData_filter(c *Data_filterContext)

	// EnterIndex_exploitation is called when entering the index_exploitation production.
	EnterIndex_exploitation(c *Index_exploitationContext)

	// EnterExploitation_rule is called when entering the exploitation_rule production.
	EnterExploitation_rule(c *Exploitation_ruleContext)

	// EnterCreate_function_external_table_statement is called when entering the create_function_external_table_statement production.
	EnterCreate_function_external_table_statement(c *Create_function_external_table_statementContext)

	// EnterExt_table_param_decl_list is called when entering the ext_table_param_decl_list production.
	EnterExt_table_param_decl_list(c *Ext_table_param_decl_listContext)

	// EnterExt_table_param_decl is called when entering the ext_table_param_decl production.
	EnterExt_table_param_decl(c *Ext_table_param_declContext)

	// EnterExt_table_option_list is called when entering the ext_table_option_list production.
	EnterExt_table_option_list(c *Ext_table_option_listContext)

	// EnterExt_table_option_list_item is called when entering the ext_table_option_list_item production.
	EnterExt_table_option_list_item(c *Ext_table_option_list_itemContext)

	// EnterCreate_function_old_db_external_function_statement is called when entering the create_function_old_db_external_function_statement production.
	EnterCreate_function_old_db_external_function_statement(c *Create_function_old_db_external_function_statementContext)

	// EnterOledb_option_list is called when entering the oledb_option_list production.
	EnterOledb_option_list(c *Oledb_option_listContext)

	// EnterOledb_option_list_item is called when entering the oledb_option_list_item production.
	EnterOledb_option_list_item(c *Oledb_option_list_itemContext)

	// EnterCreate_function_sourced_or_template_statement is called when entering the create_function_sourced_or_template_statement production.
	EnterCreate_function_sourced_or_template_statement(c *Create_function_sourced_or_template_statementContext)

	// EnterFn_return_opts is called when entering the fn_return_opts production.
	EnterFn_return_opts(c *Fn_return_optsContext)

	// EnterFn_return_opts_item is called when entering the fn_return_opts_item production.
	EnterFn_return_opts_item(c *Fn_return_opts_itemContext)

	// EnterTemplate_opts is called when entering the template_opts production.
	EnterTemplate_opts(c *Template_optsContext)

	// EnterTemplate_opts_item is called when entering the template_opts_item production.
	EnterTemplate_opts_item(c *Template_opts_itemContext)

	// EnterAscii_unicode is called when entering the ascii_unicode production.
	EnterAscii_unicode(c *Ascii_unicodeContext)

	// EnterParam_decl_list_3 is called when entering the param_decl_list_3 production.
	EnterParam_decl_list_3(c *Param_decl_list_3Context)

	// EnterParam_decl_3 is called when entering the param_decl_3 production.
	EnterParam_decl_3(c *Param_decl_3Context)

	// EnterCreate_function_sql_scalar_table_or_row_statement is called when entering the create_function_sql_scalar_table_or_row_statement production.
	EnterCreate_function_sql_scalar_table_or_row_statement(c *Create_function_sql_scalar_table_or_row_statementContext)

	// EnterParam_decl_list_2 is called when entering the param_decl_list_2 production.
	EnterParam_decl_list_2(c *Param_decl_list_2Context)

	// EnterParam_decl_2 is called when entering the param_decl_2 production.
	EnterParam_decl_2(c *Param_decl_2Context)

	// EnterSql_function_body is called when entering the sql_function_body production.
	EnterSql_function_body(c *Sql_function_bodyContext)

	// EnterCreate_function_mapping_statement is called when entering the create_function_mapping_statement production.
	EnterCreate_function_mapping_statement(c *Create_function_mapping_statementContext)

	// EnterFunction_options is called when entering the function_options production.
	EnterFunction_options(c *Function_optionsContext)

	// EnterFunction_option_name is called when entering the function_option_name production.
	EnterFunction_option_name(c *Function_option_nameContext)

	// EnterCreate_global_temporary_table_statement is called when entering the create_global_temporary_table_statement production.
	EnterCreate_global_temporary_table_statement(c *Create_global_temporary_table_statementContext)

	// EnterCreate_global_temporary_table_opts is called when entering the create_global_temporary_table_opts production.
	EnterCreate_global_temporary_table_opts(c *Create_global_temporary_table_optsContext)

	// EnterCreate_global_temporary_table_item is called when entering the create_global_temporary_table_item production.
	EnterCreate_global_temporary_table_item(c *Create_global_temporary_table_itemContext)

	// EnterDelete_preserve is called when entering the delete_preserve production.
	EnterDelete_preserve(c *Delete_preserveContext)

	// EnterCreate_histogram_template_statement is called when entering the create_histogram_template_statement production.
	EnterCreate_histogram_template_statement(c *Create_histogram_template_statementContext)

	// EnterCreate_index_statement is called when entering the create_index_statement production.
	EnterCreate_index_statement(c *Create_index_statementContext)

	// EnterIndex_col_opts is called when entering the index_col_opts production.
	EnterIndex_col_opts(c *Index_col_optsContext)

	// EnterIndex_col_opts_item is called when entering the index_col_opts_item production.
	EnterIndex_col_opts_item(c *Index_col_opts_itemContext)

	// EnterKey_expression is called when entering the key_expression production.
	EnterKey_expression(c *Key_expressionContext)

	// EnterCreate_index_extension_statement is called when entering the create_index_extension_statement production.
	EnterCreate_index_extension_statement(c *Create_index_extension_statementContext)

	// EnterParam_list is called when entering the param_list production.
	EnterParam_list(c *Param_listContext)

	// EnterIndex_maintenance is called when entering the index_maintenance production.
	EnterIndex_maintenance(c *Index_maintenanceContext)

	// EnterTable_function_invocation is called when entering the table_function_invocation production.
	EnterTable_function_invocation(c *Table_function_invocationContext)

	// EnterIndex_search is called when entering the index_search production.
	EnterIndex_search(c *Index_searchContext)

	// EnterSearch_method_definition is called when entering the search_method_definition production.
	EnterSearch_method_definition(c *Search_method_definitionContext)

	// EnterCreate_mask_statement is called when entering the create_mask_statement production.
	EnterCreate_mask_statement(c *Create_mask_statementContext)

	// EnterCase_expression is called when entering the case_expression production.
	EnterCase_expression(c *Case_expressionContext)

	// EnterRange_producing_funciton_invocation is called when entering the range_producing_funciton_invocation production.
	EnterRange_producing_funciton_invocation(c *Range_producing_funciton_invocationContext)

	// EnterIndex_filtering_function_invocation is called when entering the index_filtering_function_invocation production.
	EnterIndex_filtering_function_invocation(c *Index_filtering_function_invocationContext)

	// EnterCreate_method_statement is called when entering the create_method_statement production.
	EnterCreate_method_statement(c *Create_method_statementContext)

	// EnterMethod_opts is called when entering the method_opts production.
	EnterMethod_opts(c *Method_optsContext)

	// EnterMethod_opts_item is called when entering the method_opts_item production.
	EnterMethod_opts_item(c *Method_opts_itemContext)

	// EnterMethod_signature is called when entering the method_signature production.
	EnterMethod_signature(c *Method_signatureContext)

	// EnterMethod_param_list is called when entering the method_param_list production.
	EnterMethod_param_list(c *Method_param_listContext)

	// EnterData_type_3 is called when entering the data_type_3 production.
	EnterData_type_3(c *Data_type_3Context)

	// EnterData_type_4 is called when entering the data_type_4 production.
	EnterData_type_4(c *Data_type_4Context)

	// EnterSql_method_body is called when entering the sql_method_body production.
	EnterSql_method_body(c *Sql_method_bodyContext)

	// EnterCompound_sql_inlined is called when entering the compound_sql_inlined production.
	EnterCompound_sql_inlined(c *Compound_sql_inlinedContext)

	// EnterSql_statement_inlined is called when entering the sql_statement_inlined production.
	EnterSql_statement_inlined(c *Sql_statement_inlinedContext)

	// EnterCompound_sql_compiled is called when entering the compound_sql_compiled production.
	EnterCompound_sql_compiled(c *Compound_sql_compiledContext)

	// EnterSql_statement_compiled is called when entering the sql_statement_compiled production.
	EnterSql_statement_compiled(c *Sql_statement_compiledContext)

	// EnterCreate_module_statement is called when entering the create_module_statement production.
	EnterCreate_module_statement(c *Create_module_statementContext)

	// EnterCreate_nickname_statement is called when entering the create_nickname_statement production.
	EnterCreate_nickname_statement(c *Create_nickname_statementContext)

	// EnterNick_name_option_name is called when entering the nick_name_option_name production.
	EnterNick_name_option_name(c *Nick_name_option_nameContext)

	// EnterRemote_object_name is called when entering the remote_object_name production.
	EnterRemote_object_name(c *Remote_object_nameContext)

	// EnterNon_relational_data_definition is called when entering the non_relational_data_definition production.
	EnterNon_relational_data_definition(c *Non_relational_data_definitionContext)

	// EnterNick_name_column_list is called when entering the nick_name_column_list production.
	EnterNick_name_column_list(c *Nick_name_column_listContext)

	// EnterNick_name_column_list_item is called when entering the nick_name_column_list_item production.
	EnterNick_name_column_list_item(c *Nick_name_column_list_itemContext)

	// EnterNick_name_column_definition is called when entering the nick_name_column_definition production.
	EnterNick_name_column_definition(c *Nick_name_column_definitionContext)

	// EnterNick_name_column_options is called when entering the nick_name_column_options production.
	EnterNick_name_column_options(c *Nick_name_column_optionsContext)

	// EnterFederated_column_options is called when entering the federated_column_options production.
	EnterFederated_column_options(c *Federated_column_optionsContext)

	// EnterColumn_option_name is called when entering the column_option_name production.
	EnterColumn_option_name(c *Column_option_nameContext)

	// EnterCreate_permission_statement is called when entering the create_permission_statement production.
	EnterCreate_permission_statement(c *Create_permission_statementContext)

	// EnterCreate_procedure_statement is called when entering the create_procedure_statement production.
	EnterCreate_procedure_statement(c *Create_procedure_statementContext)

	// EnterCreate_procedure_external_statement is called when entering the create_procedure_external_statement production.
	EnterCreate_procedure_external_statement(c *Create_procedure_external_statementContext)

	// EnterProc_ext_param_list is called when entering the proc_ext_param_list production.
	EnterProc_ext_param_list(c *Proc_ext_param_listContext)

	// EnterProc_ext_param is called when entering the proc_ext_param production.
	EnterProc_ext_param(c *Proc_ext_paramContext)

	// EnterOption_list_2 is called when entering the option_list_2 production.
	EnterOption_list_2(c *Option_list_2Context)

	// EnterOption_list_2_item is called when entering the option_list_2_item production.
	EnterOption_list_2_item(c *Option_list_2_itemContext)

	// EnterCreate_procedure_sourced_statement is called when entering the create_procedure_sourced_statement production.
	EnterCreate_procedure_sourced_statement(c *Create_procedure_sourced_statementContext)

	// EnterSource_procedure_clause is called when entering the source_procedure_clause production.
	EnterSource_procedure_clause(c *Source_procedure_clauseContext)

	// EnterSource_object_name is called when entering the source_object_name production.
	EnterSource_object_name(c *Source_object_nameContext)

	// EnterOption_list_1 is called when entering the option_list_1 production.
	EnterOption_list_1(c *Option_list_1Context)

	// EnterOption_list_1_item is called when entering the option_list_1_item production.
	EnterOption_list_1_item(c *Option_list_1_itemContext)

	// EnterResult_set_element_number is called when entering the result_set_element_number production.
	EnterResult_set_element_number(c *Result_set_element_numberContext)

	// EnterUnique_id is called when entering the unique_id production.
	EnterUnique_id(c *Unique_idContext)

	// EnterCreate_procedure_sql_statement is called when entering the create_procedure_sql_statement production.
	EnterCreate_procedure_sql_statement(c *Create_procedure_sql_statementContext)

	// EnterProc_parameter_list is called when entering the proc_parameter_list production.
	EnterProc_parameter_list(c *Proc_parameter_listContext)

	// EnterProc_parameter_list_item is called when entering the proc_parameter_list_item production.
	EnterProc_parameter_list_item(c *Proc_parameter_list_itemContext)

	// EnterIn_out_inout is called when entering the in_out_inout production.
	EnterIn_out_inout(c *In_out_inoutContext)

	// EnterOption_list is called when entering the option_list production.
	EnterOption_list(c *Option_listContext)

	// EnterOption_list_item is called when entering the option_list_item production.
	EnterOption_list_item(c *Option_list_itemContext)

	// EnterSql_procedure_body is called when entering the sql_procedure_body production.
	EnterSql_procedure_body(c *Sql_procedure_bodyContext)

	// EnterCreate_role_statement is called when entering the create_role_statement production.
	EnterCreate_role_statement(c *Create_role_statementContext)

	// EnterCreate_schema_statement is called when entering the create_schema_statement production.
	EnterCreate_schema_statement(c *Create_schema_statementContext)

	// EnterSchema_sql_statement is called when entering the schema_sql_statement production.
	EnterSchema_sql_statement(c *Schema_sql_statementContext)

	// EnterCreate_security_label_component_statement is called when entering the create_security_label_component_statement production.
	EnterCreate_security_label_component_statement(c *Create_security_label_component_statementContext)

	// EnterArray_clause is called when entering the array_clause production.
	EnterArray_clause(c *Array_clauseContext)

	// EnterSet_clause is called when entering the set_clause production.
	EnterSet_clause(c *Set_clauseContext)

	// EnterTree_clause is called when entering the tree_clause production.
	EnterTree_clause(c *Tree_clauseContext)

	// EnterTree_clause_item is called when entering the tree_clause_item production.
	EnterTree_clause_item(c *Tree_clause_itemContext)

	// EnterCreate_security_label_statement is called when entering the create_security_label_statement production.
	EnterCreate_security_label_statement(c *Create_security_label_statementContext)

	// EnterCreate_security_label_item is called when entering the create_security_label_item production.
	EnterCreate_security_label_item(c *Create_security_label_itemContext)

	// EnterCreate_security_policy_statement is called when entering the create_security_policy_statement production.
	EnterCreate_security_policy_statement(c *Create_security_policy_statementContext)

	// EnterCreate_sequence_statement is called when entering the create_sequence_statement production.
	EnterCreate_sequence_statement(c *Create_sequence_statementContext)

	// EnterCreate_sequence_opts is called when entering the create_sequence_opts production.
	EnterCreate_sequence_opts(c *Create_sequence_optsContext)

	// EnterCreate_sequence_opts_item is called when entering the create_sequence_opts_item production.
	EnterCreate_sequence_opts_item(c *Create_sequence_opts_itemContext)

	// EnterCreate_service_class_statement is called when entering the create_service_class_statement production.
	EnterCreate_service_class_statement(c *Create_service_class_statementContext)

	// EnterHigh_medium_low is called when entering the high_medium_low production.
	EnterHigh_medium_low(c *High_medium_lowContext)

	// EnterOn_off is called when entering the on_off production.
	EnterOn_off(c *On_offContext)

	// EnterSoft_hard is called when entering the soft_hard production.
	EnterSoft_hard(c *Soft_hardContext)

	// EnterCreate_server_statement is called when entering the create_server_statement production.
	EnterCreate_server_statement(c *Create_server_statementContext)

	// EnterPassword_ is called when entering the password_ production.
	EnterPassword_(c *Password_Context)

	// EnterCreate_stogroup_statement is called when entering the create_stogroup_statement production.
	EnterCreate_stogroup_statement(c *Create_stogroup_statementContext)

	// EnterCreate_stogroup_opts is called when entering the create_stogroup_opts production.
	EnterCreate_stogroup_opts(c *Create_stogroup_optsContext)

	// EnterCreate_synonym_statement is called when entering the create_synonym_statement production.
	EnterCreate_synonym_statement(c *Create_synonym_statementContext)

	// EnterCreate_table_statement is called when entering the create_table_statement production.
	EnterCreate_table_statement(c *Create_table_statementContext)

	// EnterCreate_table_opts is called when entering the create_table_opts production.
	EnterCreate_table_opts(c *Create_table_optsContext)

	// EnterTable_option_list is called when entering the table_option_list production.
	EnterTable_option_list(c *Table_option_listContext)

	// EnterTable_option_list_item is called when entering the table_option_list_item production.
	EnterTable_option_list_item(c *Table_option_list_itemContext)

	// EnterTable_option_name is called when entering the table_option_name production.
	EnterTable_option_name(c *Table_option_nameContext)

	// EnterElement_list is called when entering the element_list production.
	EnterElement_list(c *Element_listContext)

	// EnterElement_list_item is called when entering the element_list_item production.
	EnterElement_list_item(c *Element_list_itemContext)

	// EnterColumn_definition is called when entering the column_definition production.
	EnterColumn_definition(c *Column_definitionContext)

	// EnterPeriod_definition is called when entering the period_definition production.
	EnterPeriod_definition(c *Period_definitionContext)

	// EnterUnique_constraint is called when entering the unique_constraint production.
	EnterUnique_constraint(c *Unique_constraintContext)

	// EnterReferential_constraint is called when entering the referential_constraint production.
	EnterReferential_constraint(c *Referential_constraintContext)

	// EnterCheck_constraint is called when entering the check_constraint production.
	EnterCheck_constraint(c *Check_constraintContext)

	// EnterColumn_options is called when entering the column_options production.
	EnterColumn_options(c *Column_optionsContext)

	// EnterColumn_options_item is called when entering the column_options_item production.
	EnterColumn_options_item(c *Column_options_itemContext)

	// EnterReferences_clause is called when entering the references_clause production.
	EnterReferences_clause(c *References_clauseContext)

	// EnterRule_clause is called when entering the rule_clause production.
	EnterRule_clause(c *Rule_clauseContext)

	// EnterConstraint_attributes is called when entering the constraint_attributes production.
	EnterConstraint_attributes(c *Constraint_attributesContext)

	// EnterDefault_clause is called when entering the default_clause production.
	EnterDefault_clause(c *Default_clauseContext)

	// EnterDefault_values is called when entering the default_values production.
	EnterDefault_values(c *Default_valuesContext)

	// EnterGenerated_clause is called when entering the generated_clause production.
	EnterGenerated_clause(c *Generated_clauseContext)

	// EnterDatetime_special_register is called when entering the datetime_special_register production.
	EnterDatetime_special_register(c *Datetime_special_registerContext)

	// EnterUser_special_register is called when entering the user_special_register production.
	EnterUser_special_register(c *User_special_registerContext)

	// EnterCast_function is called when entering the cast_function production.
	EnterCast_function(c *Cast_functionContext)

	// EnterIdentity_options is called when entering the identity_options production.
	EnterIdentity_options(c *Identity_optionsContext)

	// EnterIdentity_options_item is called when entering the identity_options_item production.
	EnterIdentity_options_item(c *Identity_options_itemContext)

	// EnterAs_row_change_timestamp_clause is called when entering the as_row_change_timestamp_clause production.
	EnterAs_row_change_timestamp_clause(c *As_row_change_timestamp_clauseContext)

	// EnterAs_generated_expression_clause is called when entering the as_generated_expression_clause production.
	EnterAs_generated_expression_clause(c *As_generated_expression_clauseContext)

	// EnterGeneration_expression is called when entering the generation_expression production.
	EnterGeneration_expression(c *Generation_expressionContext)

	// EnterAs_row_transaction_timestamp_clause is called when entering the as_row_transaction_timestamp_clause production.
	EnterAs_row_transaction_timestamp_clause(c *As_row_transaction_timestamp_clauseContext)

	// EnterAs_row_transaction_start_id_clause is called when entering the as_row_transaction_start_id_clause production.
	EnterAs_row_transaction_start_id_clause(c *As_row_transaction_start_id_clauseContext)

	// EnterOid_column_definition is called when entering the oid_column_definition production.
	EnterOid_column_definition(c *Oid_column_definitionContext)

	// EnterRange_partition_spec is called when entering the range_partition_spec production.
	EnterRange_partition_spec(c *Range_partition_specContext)

	// EnterPartition_expression_list is called when entering the partition_expression_list production.
	EnterPartition_expression_list(c *Partition_expression_listContext)

	// EnterPartition_expression is called when entering the partition_expression production.
	EnterPartition_expression(c *Partition_expressionContext)

	// EnterPartition_element_list is called when entering the partition_element_list production.
	EnterPartition_element_list(c *Partition_element_listContext)

	// EnterPartition_element is called when entering the partition_element production.
	EnterPartition_element(c *Partition_elementContext)

	// EnterBoundary_spec is called when entering the boundary_spec production.
	EnterBoundary_spec(c *Boundary_specContext)

	// EnterPartition_tablespace_options is called when entering the partition_tablespace_options production.
	EnterPartition_tablespace_options(c *Partition_tablespace_optionsContext)

	// EnterDuration_label is called when entering the duration_label production.
	EnterDuration_label(c *Duration_labelContext)

	// EnterStarting_clause is called when entering the starting_clause production.
	EnterStarting_clause(c *Starting_clauseContext)

	// EnterConst_min_max_list is called when entering the const_min_max_list production.
	EnterConst_min_max_list(c *Const_min_max_listContext)

	// EnterConst_min_max is called when entering the const_min_max production.
	EnterConst_min_max(c *Const_min_maxContext)

	// EnterEnding_clause is called when entering the ending_clause production.
	EnterEnding_clause(c *Ending_clauseContext)

	// EnterTyped_table_options is called when entering the typed_table_options production.
	EnterTyped_table_options(c *Typed_table_optionsContext)

	// EnterTyped_element_list is called when entering the typed_element_list production.
	EnterTyped_element_list(c *Typed_element_listContext)

	// EnterTyped_element_list_item is called when entering the typed_element_list_item production.
	EnterTyped_element_list_item(c *Typed_element_list_itemContext)

	// EnterAs_result_table is called when entering the as_result_table production.
	EnterAs_result_table(c *As_result_tableContext)

	// EnterCopy_options is called when entering the copy_options production.
	EnterCopy_options(c *Copy_optionsContext)

	// EnterMaterialized_query_options is called when entering the materialized_query_options production.
	EnterMaterialized_query_options(c *Materialized_query_optionsContext)

	// EnterStaging_table_definition is called when entering the staging_table_definition production.
	EnterStaging_table_definition(c *Staging_table_definitionContext)

	// EnterDimensions_clause is called when entering the dimensions_clause production.
	EnterDimensions_clause(c *Dimensions_clauseContext)

	// EnterCol_names is called when entering the col_names production.
	EnterCol_names(c *Col_namesContext)

	// EnterSequence_key_spec is called when entering the sequence_key_spec production.
	EnterSequence_key_spec(c *Sequence_key_specContext)

	// EnterSequence_key_spec_list is called when entering the sequence_key_spec_list production.
	EnterSequence_key_spec_list(c *Sequence_key_spec_listContext)

	// EnterSequence_key_spec_list_item is called when entering the sequence_key_spec_list_item production.
	EnterSequence_key_spec_list_item(c *Sequence_key_spec_list_itemContext)

	// EnterTablespace_clauses is called when entering the tablespace_clauses production.
	EnterTablespace_clauses(c *Tablespace_clausesContext)

	// EnterDistribution_clause is called when entering the distribution_clause production.
	EnterDistribution_clause(c *Distribution_clauseContext)

	// EnterPartitioning_clause is called when entering the partitioning_clause production.
	EnterPartitioning_clause(c *Partitioning_clauseContext)

	// EnterIf_not_exists is called when entering the if_not_exists production.
	EnterIf_not_exists(c *If_not_existsContext)

	// EnterCreate_tablespace_statement is called when entering the create_tablespace_statement production.
	EnterCreate_tablespace_statement(c *Create_tablespace_statementContext)

	// EnterStorage_group is called when entering the storage_group production.
	EnterStorage_group(c *Storage_groupContext)

	// EnterSize_attributes is called when entering the size_attributes production.
	EnterSize_attributes(c *Size_attributesContext)

	// EnterSystem_containers is called when entering the system_containers production.
	EnterSystem_containers(c *System_containersContext)

	// EnterContainer_string_list is called when entering the container_string_list production.
	EnterContainer_string_list(c *Container_string_listContext)

	// EnterDatabase_containers is called when entering the database_containers production.
	EnterDatabase_containers(c *Database_containersContext)

	// EnterContainer_clause is called when entering the container_clause production.
	EnterContainer_clause(c *Container_clauseContext)

	// EnterContainer_clause_list is called when entering the container_clause_list production.
	EnterContainer_clause_list(c *Container_clause_listContext)

	// EnterContainer_clause_list_item is called when entering the container_clause_list_item production.
	EnterContainer_clause_list_item(c *Container_clause_list_itemContext)

	// EnterOn_db_partitions_clause is called when entering the on_db_partitions_clause production.
	EnterOn_db_partitions_clause(c *On_db_partitions_clauseContext)

	// EnterDb_partition_number_list is called when entering the db_partition_number_list production.
	EnterDb_partition_number_list(c *Db_partition_number_listContext)

	// EnterDb_partition_number_list_item is called when entering the db_partition_number_list_item production.
	EnterDb_partition_number_list_item(c *Db_partition_number_list_itemContext)

	// EnterDb_partition_number is called when entering the db_partition_number production.
	EnterDb_partition_number(c *Db_partition_numberContext)

	// EnterNumber_of_pages is called when entering the number_of_pages production.
	EnterNumber_of_pages(c *Number_of_pagesContext)

	// EnterNumber_of_files is called when entering the number_of_files production.
	EnterNumber_of_files(c *Number_of_filesContext)

	// EnterNumber_of_milliseconds is called when entering the number_of_milliseconds production.
	EnterNumber_of_milliseconds(c *Number_of_millisecondsContext)

	// EnterNumber_megabytes_per_second is called when entering the number_megabytes_per_second production.
	EnterNumber_megabytes_per_second(c *Number_megabytes_per_secondContext)

	// EnterCreate_threshold_statement is called when entering the create_threshold_statement production.
	EnterCreate_threshold_statement(c *Create_threshold_statementContext)

	// EnterThreshold_domain is called when entering the threshold_domain production.
	EnterThreshold_domain(c *Threshold_domainContext)

	// EnterStatement_text is called when entering the statement_text production.
	EnterStatement_text(c *Statement_textContext)

	// EnterExecutable_id is called when entering the executable_id production.
	EnterExecutable_id(c *Executable_idContext)

	// EnterEnforcement_scope is called when entering the enforcement_scope production.
	EnterEnforcement_scope(c *Enforcement_scopeContext)

	// EnterThreshold_predicate is called when entering the threshold_predicate production.
	EnterThreshold_predicate(c *Threshold_predicateContext)

	// EnterChecking_every is called when entering the checking_every production.
	EnterChecking_every(c *Checking_everyContext)

	// EnterHour_to_seconds is called when entering the hour_to_seconds production.
	EnterHour_to_seconds(c *Hour_to_secondsContext)

	// EnterDay_to_minutes is called when entering the day_to_minutes production.
	EnterDay_to_minutes(c *Day_to_minutesContext)

	// EnterDay_to_seconds is called when entering the day_to_seconds production.
	EnterDay_to_seconds(c *Day_to_secondsContext)

	// EnterThreshold_exceeded_actions_2 is called when entering the threshold_exceeded_actions_2 production.
	EnterThreshold_exceeded_actions_2(c *Threshold_exceeded_actions_2Context)

	// EnterDetails_section is called when entering the details_section production.
	EnterDetails_section(c *Details_sectionContext)

	// EnterRemap_activity_action is called when entering the remap_activity_action production.
	EnterRemap_activity_action(c *Remap_activity_actionContext)

	// EnterCreate_transform_statement is called when entering the create_transform_statement production.
	EnterCreate_transform_statement(c *Create_transform_statementContext)

	// EnterTranform_list is called when entering the tranform_list production.
	EnterTranform_list(c *Tranform_listContext)

	// EnterTranform_list_item is called when entering the tranform_list_item production.
	EnterTranform_list_item(c *Tranform_list_itemContext)

	// EnterTransform_group_list is called when entering the transform_group_list production.
	EnterTransform_group_list(c *Transform_group_listContext)

	// EnterTransform_group_list_item is called when entering the transform_group_list_item production.
	EnterTransform_group_list_item(c *Transform_group_list_itemContext)

	// EnterCreate_trigger_statement is called when entering the create_trigger_statement production.
	EnterCreate_trigger_statement(c *Create_trigger_statementContext)

	// EnterRef_list is called when entering the ref_list production.
	EnterRef_list(c *Ref_listContext)

	// EnterRef_list_item is called when entering the ref_list_item production.
	EnterRef_list_item(c *Ref_list_itemContext)

	// EnterOld_new is called when entering the old_new production.
	EnterOld_new(c *Old_newContext)

	// EnterCorrelation_name is called when entering the correlation_name production.
	EnterCorrelation_name(c *Correlation_nameContext)

	// EnterIdentifier is called when entering the identifier production.
	EnterIdentifier(c *IdentifierContext)

	// EnterTrigger_event is called when entering the trigger_event production.
	EnterTrigger_event(c *Trigger_eventContext)

	// EnterTriggered_action is called when entering the triggered_action production.
	EnterTriggered_action(c *Triggered_actionContext)

	// EnterSql_procedure_statement is called when entering the sql_procedure_statement production.
	EnterSql_procedure_statement(c *Sql_procedure_statementContext)

	// EnterSql_function_statement is called when entering the sql_function_statement production.
	EnterSql_function_statement(c *Sql_function_statementContext)

	// EnterCreate_trusted_context_statement is called when entering the create_trusted_context_statement production.
	EnterCreate_trusted_context_statement(c *Create_trusted_context_statementContext)

	// EnterAttr_list is called when entering the attr_list production.
	EnterAttr_list(c *Attr_listContext)

	// EnterAttr_list_item is called when entering the attr_list_item production.
	EnterAttr_list_item(c *Attr_list_itemContext)

	// EnterAuth_list is called when entering the auth_list production.
	EnterAuth_list(c *Auth_listContext)

	// EnterAuth_list_item is called when entering the auth_list_item production.
	EnterAuth_list_item(c *Auth_list_itemContext)

	// EnterAddress_value is called when entering the address_value production.
	EnterAddress_value(c *Address_valueContext)

	// EnterEncryption_value is called when entering the encryption_value production.
	EnterEncryption_value(c *Encryption_valueContext)

	// EnterCreate_type_statement is called when entering the create_type_statement production.
	EnterCreate_type_statement(c *Create_type_statementContext)

	// EnterCreate_type_array_statement is called when entering the create_type_array_statement production.
	EnterCreate_type_array_statement(c *Create_type_array_statementContext)

	// EnterCreate_type_cursor_statement is called when entering the create_type_cursor_statement production.
	EnterCreate_type_cursor_statement(c *Create_type_cursor_statementContext)

	// EnterCreate_type_distinct_statement is called when entering the create_type_distinct_statement production.
	EnterCreate_type_distinct_statement(c *Create_type_distinct_statementContext)

	// EnterCreate_type_row_statement is called when entering the create_type_row_statement production.
	EnterCreate_type_row_statement(c *Create_type_row_statementContext)

	// EnterField_definition_list_paren is called when entering the field_definition_list_paren production.
	EnterField_definition_list_paren(c *Field_definition_list_parenContext)

	// EnterField_definition_list is called when entering the field_definition_list production.
	EnterField_definition_list(c *Field_definition_listContext)

	// EnterField_definition is called when entering the field_definition production.
	EnterField_definition(c *Field_definitionContext)

	// EnterCreate_type_structured_statement is called when entering the create_type_structured_statement production.
	EnterCreate_type_structured_statement(c *Create_type_structured_statementContext)

	// EnterStructured_type_seq is called when entering the structured_type_seq production.
	EnterStructured_type_seq(c *Structured_type_seqContext)

	// EnterAttribute_definition_list_paren is called when entering the attribute_definition_list_paren production.
	EnterAttribute_definition_list_paren(c *Attribute_definition_list_parenContext)

	// EnterAttribute_definition_list is called when entering the attribute_definition_list production.
	EnterAttribute_definition_list(c *Attribute_definition_listContext)

	// EnterAttribute_definition is called when entering the attribute_definition production.
	EnterAttribute_definition(c *Attribute_definitionContext)

	// EnterMethod_specification_list is called when entering the method_specification_list production.
	EnterMethod_specification_list(c *Method_specification_listContext)

	// EnterMethod_specification is called when entering the method_specification production.
	EnterMethod_specification(c *Method_specificationContext)

	// EnterMethod_specification_seq is called when entering the method_specification_seq production.
	EnterMethod_specification_seq(c *Method_specification_seqContext)

	// EnterAs_locator is called when entering the as_locator production.
	EnterAs_locator(c *As_locatorContext)

	// EnterParam_decl_list_paren is called when entering the param_decl_list_paren production.
	EnterParam_decl_list_paren(c *Param_decl_list_parenContext)

	// EnterParam_decl_list is called when entering the param_decl_list production.
	EnterParam_decl_list(c *Param_decl_listContext)

	// EnterParam_decl is called when entering the param_decl production.
	EnterParam_decl(c *Param_declContext)

	// EnterSql_routine_characteristics is called when entering the sql_routine_characteristics production.
	EnterSql_routine_characteristics(c *Sql_routine_characteristicsContext)

	// EnterExternal_routine_characteristics is called when entering the external_routine_characteristics production.
	EnterExternal_routine_characteristics(c *External_routine_characteristicsContext)

	// EnterLength is called when entering the length production.
	EnterLength(c *LengthContext)

	// EnterRep_type is called when entering the rep_type production.
	EnterRep_type(c *Rep_typeContext)

	// EnterVarchars is called when entering the varchars production.
	EnterVarchars(c *VarcharsContext)

	// EnterVarbinaries is called when entering the varbinaries production.
	EnterVarbinaries(c *VarbinariesContext)

	// EnterFor_bit_data is called when entering the for_bit_data production.
	EnterFor_bit_data(c *For_bit_dataContext)

	// EnterLob_options is called when entering the lob_options production.
	EnterLob_options(c *Lob_optionsContext)

	// EnterCreate_type_mapping_statement is called when entering the create_type_mapping_statement production.
	EnterCreate_type_mapping_statement(c *Create_type_mapping_statementContext)

	// EnterFor_bit_data_precision is called when entering the for_bit_data_precision production.
	EnterFor_bit_data_precision(c *For_bit_data_precisionContext)

	// EnterPrecision is called when entering the precision production.
	EnterPrecision(c *PrecisionContext)

	// EnterScale is called when entering the scale production.
	EnterScale(c *ScaleContext)

	// EnterPrecision_scale_comp is called when entering the precision_scale_comp production.
	EnterPrecision_scale_comp(c *Precision_scale_compContext)

	// EnterFrom_to is called when entering the from_to production.
	EnterFrom_to(c *From_toContext)

	// EnterData_source_data_type is called when entering the data_source_data_type production.
	EnterData_source_data_type(c *Data_source_data_typeContext)

	// EnterLocal_data_type is called when entering the local_data_type production.
	EnterLocal_data_type(c *Local_data_typeContext)

	// EnterRemote_server is called when entering the remote_server production.
	EnterRemote_server(c *Remote_serverContext)

	// EnterServer_version is called when entering the server_version production.
	EnterServer_version(c *Server_versionContext)

	// EnterServer_type is called when entering the server_type production.
	EnterServer_type(c *Server_typeContext)

	// EnterVersion is called when entering the version production.
	EnterVersion(c *VersionContext)

	// EnterRelease is called when entering the release production.
	EnterRelease(c *ReleaseContext)

	// EnterMod is called when entering the mod production.
	EnterMod(c *ModContext)

	// EnterCreate_usage_list_statement is called when entering the create_usage_list_statement production.
	EnterCreate_usage_list_statement(c *Create_usage_list_statementContext)

	// EnterCreate_user_mapping_statement is called when entering the create_user_mapping_statement production.
	EnterCreate_user_mapping_statement(c *Create_user_mapping_statementContext)

	// EnterUser_mapping_options_paren is called when entering the user_mapping_options_paren production.
	EnterUser_mapping_options_paren(c *User_mapping_options_parenContext)

	// EnterUser_mapping_options is called when entering the user_mapping_options production.
	EnterUser_mapping_options(c *User_mapping_optionsContext)

	// EnterCreate_variable_statement is called when entering the create_variable_statement production.
	EnterCreate_variable_statement(c *Create_variable_statementContext)

	// EnterConstant_ is called when entering the constant_ production.
	EnterConstant_(c *Constant_Context)

	// EnterSpecial_register is called when entering the special_register production.
	EnterSpecial_register(c *Special_registerContext)

	// EnterGlobal_variable is called when entering the global_variable production.
	EnterGlobal_variable(c *Global_variableContext)

	// EnterData_type_1 is called when entering the data_type_1 production.
	EnterData_type_1(c *Data_type_1Context)

	// EnterCursor_value_constructor is called when entering the cursor_value_constructor production.
	EnterCursor_value_constructor(c *Cursor_value_constructorContext)

	// EnterAnchored_variable_data_type is called when entering the anchored_variable_data_type production.
	EnterAnchored_variable_data_type(c *Anchored_variable_data_typeContext)

	// EnterHoldability is called when entering the holdability production.
	EnterHoldability(c *HoldabilityContext)

	// EnterReturnability is called when entering the returnability production.
	EnterReturnability(c *ReturnabilityContext)

	// EnterCreate_view_statement is called when entering the create_view_statement production.
	EnterCreate_view_statement(c *Create_view_statementContext)

	// EnterCreate_view_seq is called when entering the create_view_seq production.
	EnterCreate_view_seq(c *Create_view_seqContext)

	// EnterFullselect is called when entering the fullselect production.
	EnterFullselect(c *FullselectContext)

	// EnterSubselect is called when entering the subselect production.
	EnterSubselect(c *SubselectContext)

	// EnterSelect_clause is called when entering the select_clause production.
	EnterSelect_clause(c *Select_clauseContext)

	// EnterSelect_clause_item is called when entering the select_clause_item production.
	EnterSelect_clause_item(c *Select_clause_itemContext)

	// EnterFrom_clause is called when entering the from_clause production.
	EnterFrom_clause(c *From_clauseContext)

	// EnterTable_reference is called when entering the table_reference production.
	EnterTable_reference(c *Table_referenceContext)

	// EnterTable_reference_list is called when entering the table_reference_list production.
	EnterTable_reference_list(c *Table_reference_listContext)

	// EnterSingles_table_reference is called when entering the singles_table_reference production.
	EnterSingles_table_reference(c *Singles_table_referenceContext)

	// EnterPeriod_specification is called when entering the period_specification production.
	EnterPeriod_specification(c *Period_specificationContext)

	// EnterValue is called when entering the value production.
	EnterValue(c *ValueContext)

	// EnterCorrelation_clause is called when entering the correlation_clause production.
	EnterCorrelation_clause(c *Correlation_clauseContext)

	// EnterTablesample_clause is called when entering the tablesample_clause production.
	EnterTablesample_clause(c *Tablesample_clauseContext)

	// EnterNumeric_expression is called when entering the numeric_expression production.
	EnterNumeric_expression(c *Numeric_expressionContext)

	// EnterSingle_view_reference is called when entering the single_view_reference production.
	EnterSingle_view_reference(c *Single_view_referenceContext)

	// EnterSingle_nickname_reference is called when entering the single_nickname_reference production.
	EnterSingle_nickname_reference(c *Single_nickname_referenceContext)

	// EnterOnly_table_reference is called when entering the only_table_reference production.
	EnterOnly_table_reference(c *Only_table_referenceContext)

	// EnterOuter_table_reference is called when entering the outer_table_reference production.
	EnterOuter_table_reference(c *Outer_table_referenceContext)

	// EnterAnalyze_table_reference is called when entering the analyze_table_reference production.
	EnterAnalyze_table_reference(c *Analyze_table_referenceContext)

	// EnterImplementation_clause is called when entering the implementation_clause production.
	EnterImplementation_clause(c *Implementation_clauseContext)

	// EnterNested_table_reference is called when entering the nested_table_reference production.
	EnterNested_table_reference(c *Nested_table_referenceContext)

	// EnterContinue_handler is called when entering the continue_handler production.
	EnterContinue_handler(c *Continue_handlerContext)

	// EnterSpecific_condition_value is called when entering the specific_condition_value production.
	EnterSpecific_condition_value(c *Specific_condition_valueContext)

	// EnterData_change_table_reference is called when entering the data_change_table_reference production.
	EnterData_change_table_reference(c *Data_change_table_referenceContext)

	// EnterSearched_update_statement is called when entering the searched_update_statement production.
	EnterSearched_update_statement(c *Searched_update_statementContext)

	// EnterSearched_delete_statement is called when entering the searched_delete_statement production.
	EnterSearched_delete_statement(c *Searched_delete_statementContext)

	// EnterFinal_new is called when entering the final_new production.
	EnterFinal_new(c *Final_newContext)

	// EnterFinal_new_old is called when entering the final_new_old production.
	EnterFinal_new_old(c *Final_new_oldContext)

	// EnterTable_function_reference is called when entering the table_function_reference production.
	EnterTable_function_reference(c *Table_function_referenceContext)

	// EnterTable_udf_cardinality_clause is called when entering the table_udf_cardinality_clause production.
	EnterTable_udf_cardinality_clause(c *Table_udf_cardinality_clauseContext)

	// EnterTyped_correlation_clause is called when entering the typed_correlation_clause production.
	EnterTyped_correlation_clause(c *Typed_correlation_clauseContext)

	// EnterColumn_name_data_type is called when entering the column_name_data_type production.
	EnterColumn_name_data_type(c *Column_name_data_typeContext)

	// EnterCollection_derived_table is called when entering the collection_derived_table production.
	EnterCollection_derived_table(c *Collection_derived_tableContext)

	// EnterTable_function is called when entering the table_function production.
	EnterTable_function(c *Table_functionContext)

	// EnterXmltable_expression is called when entering the xmltable_expression production.
	EnterXmltable_expression(c *Xmltable_expressionContext)

	// EnterXmltable_function is called when entering the xmltable_function production.
	EnterXmltable_function(c *Xmltable_functionContext)

	// EnterJoined_table is called when entering the joined_table production.
	EnterJoined_table(c *Joined_tableContext)

	// EnterJoin_condition is called when entering the join_condition production.
	EnterJoin_condition(c *Join_conditionContext)

	// EnterOuter is called when entering the outer production.
	EnterOuter(c *OuterContext)

	// EnterExternal_table_reference is called when entering the external_table_reference production.
	EnterExternal_table_reference(c *External_table_referenceContext)

	// EnterColumn_definition_2 is called when entering the column_definition_2 production.
	EnterColumn_definition_2(c *Column_definition_2Context)

	// EnterFile_name is called when entering the file_name production.
	EnterFile_name(c *File_nameContext)

	// EnterWhere_clause is called when entering the where_clause production.
	EnterWhere_clause(c *Where_clauseContext)

	// EnterGroup_by_clause is called when entering the group_by_clause production.
	EnterGroup_by_clause(c *Group_by_clauseContext)

	// EnterGroup_by_clause_opts is called when entering the group_by_clause_opts production.
	EnterGroup_by_clause_opts(c *Group_by_clause_optsContext)

	// EnterGrouping_expression is called when entering the grouping_expression production.
	EnterGrouping_expression(c *Grouping_expressionContext)

	// EnterGrouping_sets is called when entering the grouping_sets production.
	EnterGrouping_sets(c *Grouping_setsContext)

	// EnterSuper_groups is called when entering the super_groups production.
	EnterSuper_groups(c *Super_groupsContext)

	// EnterGrant_total is called when entering the grant_total production.
	EnterGrant_total(c *Grant_totalContext)

	// EnterHaving_clause is called when entering the having_clause production.
	EnterHaving_clause(c *Having_clauseContext)

	// EnterOrder_by_clause is called when entering the order_by_clause production.
	EnterOrder_by_clause(c *Order_by_clauseContext)

	// EnterOrder_by_clause_opts is called when entering the order_by_clause_opts production.
	EnterOrder_by_clause_opts(c *Order_by_clause_optsContext)

	// EnterTable_designator is called when entering the table_designator production.
	EnterTable_designator(c *Table_designatorContext)

	// EnterAsc_desc is called when entering the asc_desc production.
	EnterAsc_desc(c *Asc_descContext)

	// EnterFirst_last is called when entering the first_last production.
	EnterFirst_last(c *First_lastContext)

	// EnterSort_key is called when entering the sort_key production.
	EnterSort_key(c *Sort_keyContext)

	// EnterSimple_column_name is called when entering the simple_column_name production.
	EnterSimple_column_name(c *Simple_column_nameContext)

	// EnterSimple_integer is called when entering the simple_integer production.
	EnterSimple_integer(c *Simple_integerContext)

	// EnterSork_key_expression is called when entering the sork_key_expression production.
	EnterSork_key_expression(c *Sork_key_expressionContext)

	// EnterOffset_clause is called when entering the offset_clause production.
	EnterOffset_clause(c *Offset_clauseContext)

	// EnterOffset_row_count is called when entering the offset_row_count production.
	EnterOffset_row_count(c *Offset_row_countContext)

	// EnterFetch_clause is called when entering the fetch_clause production.
	EnterFetch_clause(c *Fetch_clauseContext)

	// EnterFetch_row_count is called when entering the fetch_row_count production.
	EnterFetch_row_count(c *Fetch_row_countContext)

	// EnterRow_rows is called when entering the row_rows production.
	EnterRow_rows(c *Row_rowsContext)

	// EnterIsolation_clause is called when entering the isolation_clause production.
	EnterIsolation_clause(c *Isolation_clauseContext)

	// EnterLock_request_clause is called when entering the lock_request_clause production.
	EnterLock_request_clause(c *Lock_request_clauseContext)

	// EnterValues_clause is called when entering the values_clause production.
	EnterValues_clause(c *Values_clauseContext)

	// EnterValues_row is called when entering the values_row production.
	EnterValues_row(c *Values_rowContext)

	// EnterRoot_view_definition is called when entering the root_view_definition production.
	EnterRoot_view_definition(c *Root_view_definitionContext)

	// EnterSubview_definition is called when entering the subview_definition production.
	EnterSubview_definition(c *Subview_definitionContext)

	// EnterOid_column is called when entering the oid_column production.
	EnterOid_column(c *Oid_columnContext)

	// EnterWith_options is called when entering the with_options production.
	EnterWith_options(c *With_optionsContext)

	// EnterWith_option_def is called when entering the with_option_def production.
	EnterWith_option_def(c *With_option_defContext)

	// EnterWith_option_scope_def is called when entering the with_option_scope_def production.
	EnterWith_option_scope_def(c *With_option_scope_defContext)

	// EnterUnder_clause is called when entering the under_clause production.
	EnterUnder_clause(c *Under_clauseContext)

	// EnterCreate_work_action_set_statement is called when entering the create_work_action_set_statement production.
	EnterCreate_work_action_set_statement(c *Create_work_action_set_statementContext)

	// EnterWork_action_definition_list_paren is called when entering the work_action_definition_list_paren production.
	EnterWork_action_definition_list_paren(c *Work_action_definition_list_parenContext)

	// EnterWork_action_definition_list is called when entering the work_action_definition_list production.
	EnterWork_action_definition_list(c *Work_action_definition_listContext)

	// EnterWork_action_definition is called when entering the work_action_definition production.
	EnterWork_action_definition(c *Work_action_definitionContext)

	// EnterAction_types_clause is called when entering the action_types_clause production.
	EnterAction_types_clause(c *Action_types_clauseContext)

	// EnterThreshold_types_clause is called when entering the threshold_types_clause production.
	EnterThreshold_types_clause(c *Threshold_types_clauseContext)

	// EnterSecond_seconds is called when entering the second_seconds production.
	EnterSecond_seconds(c *Second_secondsContext)

	// EnterHours_minutes is called when entering the hours_minutes production.
	EnterHours_minutes(c *Hours_minutesContext)

	// EnterThreshold_exceeded_actions is called when entering the threshold_exceeded_actions production.
	EnterThreshold_exceeded_actions(c *Threshold_exceeded_actionsContext)

	// EnterCollect_activity_data_clause is called when entering the collect_activity_data_clause production.
	EnterCollect_activity_data_clause(c *Collect_activity_data_clauseContext)

	// EnterWith_without is called when entering the with_without production.
	EnterWith_without(c *With_withoutContext)

	// EnterHistogram_templace_clause is called when entering the histogram_templace_clause production.
	EnterHistogram_templace_clause(c *Histogram_templace_clauseContext)

	// EnterCreate_work_class_set_statement is called when entering the create_work_class_set_statement production.
	EnterCreate_work_class_set_statement(c *Create_work_class_set_statementContext)

	// EnterWork_class_definition_list_paren is called when entering the work_class_definition_list_paren production.
	EnterWork_class_definition_list_paren(c *Work_class_definition_list_parenContext)

	// EnterWork_class_definition_list is called when entering the work_class_definition_list production.
	EnterWork_class_definition_list(c *Work_class_definition_listContext)

	// EnterWork_class_definition is called when entering the work_class_definition production.
	EnterWork_class_definition(c *Work_class_definitionContext)

	// EnterWork_attributes is called when entering the work_attributes production.
	EnterWork_attributes(c *Work_attributesContext)

	// EnterPosition_clause is called when entering the position_clause production.
	EnterPosition_clause(c *Position_clauseContext)

	// EnterPosition_ is called when entering the position_ production.
	EnterPosition_(c *Position_Context)

	// EnterFor_from_to_clause is called when entering the for_from_to_clause production.
	EnterFor_from_to_clause(c *For_from_to_clauseContext)

	// EnterFrom_value is called when entering the from_value production.
	EnterFrom_value(c *From_valueContext)

	// EnterTo_value is called when entering the to_value production.
	EnterTo_value(c *To_valueContext)

	// EnterData_tag_clause is called when entering the data_tag_clause production.
	EnterData_tag_clause(c *Data_tag_clauseContext)

	// EnterSchema_clause is called when entering the schema_clause production.
	EnterSchema_clause(c *Schema_clauseContext)

	// EnterCreate_workload_statement is called when entering the create_workload_statement production.
	EnterCreate_workload_statement(c *Create_workload_statementContext)

	// EnterPkg_exec_seq is called when entering the pkg_exec_seq production.
	EnterPkg_exec_seq(c *Pkg_exec_seqContext)

	// EnterPosition_clause_2 is called when entering the position_clause_2 production.
	EnterPosition_clause_2(c *Position_clause_2Context)

	// EnterConnection_attributes is called when entering the connection_attributes production.
	EnterConnection_attributes(c *Connection_attributesContext)

	// EnterString_list is called when entering the string_list production.
	EnterString_list(c *String_listContext)

	// EnterString_list_paren is called when entering the string_list_paren production.
	EnterString_list_paren(c *String_list_parenContext)

	// EnterWorkload_attributes is called when entering the workload_attributes production.
	EnterWorkload_attributes(c *Workload_attributesContext)

	// EnterDegree is called when entering the degree production.
	EnterDegree(c *DegreeContext)

	// EnterAllow_disallow is called when entering the allow_disallow production.
	EnterAllow_disallow(c *Allow_disallowContext)

	// EnterCollect_on_clause is called when entering the collect_on_clause production.
	EnterCollect_on_clause(c *Collect_on_clauseContext)

	// EnterCollect_details_clause is called when entering the collect_details_clause production.
	EnterCollect_details_clause(c *Collect_details_clauseContext)

	// EnterCollect_lock_wait_options is called when entering the collect_lock_wait_options production.
	EnterCollect_lock_wait_options(c *Collect_lock_wait_optionsContext)

	// EnterWait_time is called when entering the wait_time production.
	EnterWait_time(c *Wait_timeContext)

	// EnterCreate_wrapper_statement is called when entering the create_wrapper_statement production.
	EnterCreate_wrapper_statement(c *Create_wrapper_statementContext)

	// EnterWrapper_option_list is called when entering the wrapper_option_list production.
	EnterWrapper_option_list(c *Wrapper_option_listContext)

	// EnterWrapper_option is called when entering the wrapper_option production.
	EnterWrapper_option(c *Wrapper_optionContext)

	// EnterExpression is called when entering the expression production.
	EnterExpression(c *ExpressionContext)

	// EnterFunction_invocation is called when entering the function_invocation production.
	EnterFunction_invocation(c *Function_invocationContext)

	// EnterAll_distinct is called when entering the all_distinct production.
	EnterAll_distinct(c *All_distinctContext)

	// EnterScalar_fullselect is called when entering the scalar_fullselect production.
	EnterScalar_fullselect(c *Scalar_fullselectContext)

	// EnterCast_specification is called when entering the cast_specification production.
	EnterCast_specification(c *Cast_specificationContext)

	// EnterCursor_cast_specification is called when entering the cursor_cast_specification production.
	EnterCursor_cast_specification(c *Cursor_cast_specificationContext)

	// EnterRow_cast_specification is called when entering the row_cast_specification production.
	EnterRow_cast_specification(c *Row_cast_specificationContext)

	// EnterInterval_cast_specification is called when entering the interval_cast_specification production.
	EnterInterval_cast_specification(c *Interval_cast_specificationContext)

	// EnterXmlcast_specification is called when entering the xmlcast_specification production.
	EnterXmlcast_specification(c *Xmlcast_specificationContext)

	// EnterArray_element_specification is called when entering the array_element_specification production.
	EnterArray_element_specification(c *Array_element_specificationContext)

	// EnterArray_constructor is called when entering the array_constructor production.
	EnterArray_constructor(c *Array_constructorContext)

	// EnterMethod_invocation is called when entering the method_invocation production.
	EnterMethod_invocation(c *Method_invocationContext)

	// EnterOlap_specification is called when entering the olap_specification production.
	EnterOlap_specification(c *Olap_specificationContext)

	// EnterOrdered_olap_specification is called when entering the ordered_olap_specification production.
	EnterOrdered_olap_specification(c *Ordered_olap_specificationContext)

	// EnterWindow_partition_clause is called when entering the window_partition_clause production.
	EnterWindow_partition_clause(c *Window_partition_clauseContext)

	// EnterWindow_order_clause is called when entering the window_order_clause production.
	EnterWindow_order_clause(c *Window_order_clauseContext)

	// EnterNumbering_specification is called when entering the numbering_specification production.
	EnterNumbering_specification(c *Numbering_specificationContext)

	// EnterAggregation_specification is called when entering the aggregation_specification production.
	EnterAggregation_specification(c *Aggregation_specificationContext)

	// EnterOlap_aggregate_function is called when entering the olap_aggregate_function production.
	EnterOlap_aggregate_function(c *Olap_aggregate_functionContext)

	// EnterFirst_value_function is called when entering the first_value_function production.
	EnterFirst_value_function(c *First_value_functionContext)

	// EnterLast_value_function is called when entering the last_value_function production.
	EnterLast_value_function(c *Last_value_functionContext)

	// EnterNth_value_function is called when entering the nth_value_function production.
	EnterNth_value_function(c *Nth_value_functionContext)

	// EnterRatio_to_report_function is called when entering the ratio_to_report_function production.
	EnterRatio_to_report_function(c *Ratio_to_report_functionContext)

	// EnterIgnore_respect_nulls is called when entering the ignore_respect_nulls production.
	EnterIgnore_respect_nulls(c *Ignore_respect_nullsContext)

	// EnterFrom_first_last is called when entering the from_first_last production.
	EnterFrom_first_last(c *From_first_lastContext)

	// EnterWindow_aggregation_group_clause is called when entering the window_aggregation_group_clause production.
	EnterWindow_aggregation_group_clause(c *Window_aggregation_group_clauseContext)

	// EnterGroup_start is called when entering the group_start production.
	EnterGroup_start(c *Group_startContext)

	// EnterGroup_between is called when entering the group_between production.
	EnterGroup_between(c *Group_betweenContext)

	// EnterGroup_bound1 is called when entering the group_bound1 production.
	EnterGroup_bound1(c *Group_bound1Context)

	// EnterGroup_bound2 is called when entering the group_bound2 production.
	EnterGroup_bound2(c *Group_bound2Context)

	// EnterGroup_end is called when entering the group_end production.
	EnterGroup_end(c *Group_endContext)

	// EnterRow_change_expression is called when entering the row_change_expression production.
	EnterRow_change_expression(c *Row_change_expressionContext)

	// EnterSequence_reference is called when entering the sequence_reference production.
	EnterSequence_reference(c *Sequence_referenceContext)

	// EnterSubtype_treatment is called when entering the subtype_treatment production.
	EnterSubtype_treatment(c *Subtype_treatmentContext)

	// EnterExpression_list is called when entering the expression_list production.
	EnterExpression_list(c *Expression_listContext)

	// EnterExpression_list_in_parentheses is called when entering the expression_list_in_parentheses production.
	EnterExpression_list_in_parentheses(c *Expression_list_in_parenthesesContext)

	// EnterId_ is called when entering the id_ production.
	EnterId_(c *Id_Context)

	// EnterExposed_name is called when entering the exposed_name production.
	EnterExposed_name(c *Exposed_nameContext)

	// EnterName is called when entering the name production.
	EnterName(c *NameContext)

	// EnterLabel is called when entering the label production.
	EnterLabel(c *LabelContext)

	// EnterHost_label is called when entering the host_label production.
	EnterHost_label(c *Host_labelContext)

	// EnterLibrary_name is called when entering the library_name production.
	EnterLibrary_name(c *Library_nameContext)

	// EnterArray_type_name is called when entering the array_type_name production.
	EnterArray_type_name(c *Array_type_nameContext)

	// EnterAttribute_name is called when entering the attribute_name production.
	EnterAttribute_name(c *Attribute_nameContext)

	// EnterRow_type_name is called when entering the row_type_name production.
	EnterRow_type_name(c *Row_type_nameContext)

	// EnterAuthorization_name is called when entering the authorization_name production.
	EnterAuthorization_name(c *Authorization_nameContext)

	// EnterBoolean_variable_name is called when entering the boolean_variable_name production.
	EnterBoolean_variable_name(c *Boolean_variable_nameContext)

	// EnterArray_variable_name is called when entering the array_variable_name production.
	EnterArray_variable_name(c *Array_variable_nameContext)

	// EnterColumn_name is called when entering the column_name production.
	EnterColumn_name(c *Column_nameContext)

	// EnterConstraint_name is called when entering the constraint_name production.
	EnterConstraint_name(c *Constraint_nameContext)

	// EnterDescriptor_name is called when entering the descriptor_name production.
	EnterDescriptor_name(c *Descriptor_nameContext)

	// EnterDistinct_type_name is called when entering the distinct_type_name production.
	EnterDistinct_type_name(c *Distinct_type_nameContext)

	// EnterCursor_name is called when entering the cursor_name production.
	EnterCursor_name(c *Cursor_nameContext)

	// EnterCursor_type_name is called when entering the cursor_type_name production.
	EnterCursor_type_name(c *Cursor_type_nameContext)

	// EnterCondition_name is called when entering the condition_name production.
	EnterCondition_name(c *Condition_nameContext)

	// EnterData_source_name is called when entering the data_source_name production.
	EnterData_source_name(c *Data_source_nameContext)

	// EnterExpression_name is called when entering the expression_name production.
	EnterExpression_name(c *Expression_nameContext)

	// EnterGroup_name is called when entering the group_name production.
	EnterGroup_name(c *Group_nameContext)

	// EnterPolicy_name is called when entering the policy_name production.
	EnterPolicy_name(c *Policy_nameContext)

	// EnterBufferpool_name is called when entering the bufferpool_name production.
	EnterBufferpool_name(c *Bufferpool_nameContext)

	// EnterDb_partition_name is called when entering the db_partition_name production.
	EnterDb_partition_name(c *Db_partition_nameContext)

	// EnterDatabase_name is called when entering the database_name production.
	EnterDatabase_name(c *Database_nameContext)

	// EnterEvent_monitor_name is called when entering the event_monitor_name production.
	EnterEvent_monitor_name(c *Event_monitor_nameContext)

	// EnterField_name is called when entering the field_name production.
	EnterField_name(c *Field_nameContext)

	// EnterFor_loop_name is called when entering the for_loop_name production.
	EnterFor_loop_name(c *For_loop_nameContext)

	// EnterFunction_name is called when entering the function_name production.
	EnterFunction_name(c *Function_nameContext)

	// EnterFunction_mapping_name is called when entering the function_mapping_name production.
	EnterFunction_mapping_name(c *Function_mapping_nameContext)

	// EnterGlobal_variable_name is called when entering the global_variable_name production.
	EnterGlobal_variable_name(c *Global_variable_nameContext)

	// EnterHierarchy_name is called when entering the hierarchy_name production.
	EnterHierarchy_name(c *Hierarchy_nameContext)

	// EnterHost_variable_name is called when entering the host_variable_name production.
	EnterHost_variable_name(c *Host_variable_nameContext)

	// EnterParameter_marker is called when entering the parameter_marker production.
	EnterParameter_marker(c *Parameter_markerContext)

	// EnterTemplate_name is called when entering the template_name production.
	EnterTemplate_name(c *Template_nameContext)

	// EnterIndex_name is called when entering the index_name production.
	EnterIndex_name(c *Index_nameContext)

	// EnterIndex_extension_name is called when entering the index_extension_name production.
	EnterIndex_extension_name(c *Index_extension_nameContext)

	// EnterInput_descriptor_name is called when entering the input_descriptor_name production.
	EnterInput_descriptor_name(c *Input_descriptor_nameContext)

	// EnterMask_name is called when entering the mask_name production.
	EnterMask_name(c *Mask_nameContext)

	// EnterMethod_name is called when entering the method_name production.
	EnterMethod_name(c *Method_nameContext)

	// EnterModel_name is called when entering the model_name production.
	EnterModel_name(c *Model_nameContext)

	// EnterModule_name is called when entering the module_name production.
	EnterModule_name(c *Module_nameContext)

	// EnterNew_owner is called when entering the new_owner production.
	EnterNew_owner(c *New_ownerContext)

	// EnterNick_name is called when entering the nick_name production.
	EnterNick_name(c *Nick_nameContext)

	// EnterObject_name is called when entering the object_name production.
	EnterObject_name(c *Object_nameContext)

	// EnterOid_column_name is called when entering the oid_column_name production.
	EnterOid_column_name(c *Oid_column_nameContext)

	// EnterOptimization_profile_name is called when entering the optimization_profile_name production.
	EnterOptimization_profile_name(c *Optimization_profile_nameContext)

	// EnterPackage_name is called when entering the package_name production.
	EnterPackage_name(c *Package_nameContext)

	// EnterPartition_name is called when entering the partition_name production.
	EnterPartition_name(c *Partition_nameContext)

	// EnterPath_name is called when entering the path_name production.
	EnterPath_name(c *Path_nameContext)

	// EnterPermission_name is called when entering the permission_name production.
	EnterPermission_name(c *Permission_nameContext)

	// EnterPipe_name is called when entering the pipe_name production.
	EnterPipe_name(c *Pipe_nameContext)

	// EnterProcedure_name is called when entering the procedure_name production.
	EnterProcedure_name(c *Procedure_nameContext)

	// EnterResult_descriptor_name is called when entering the result_descriptor_name production.
	EnterResult_descriptor_name(c *Result_descriptor_nameContext)

	// EnterRole_name is called when entering the role_name production.
	EnterRole_name(c *Role_nameContext)

	// EnterRoot_table_name is called when entering the root_table_name production.
	EnterRoot_table_name(c *Root_table_nameContext)

	// EnterRoot_view_name is called when entering the root_view_name production.
	EnterRoot_view_name(c *Root_view_nameContext)

	// EnterRow_variable_name is called when entering the row_variable_name production.
	EnterRow_variable_name(c *Row_variable_nameContext)

	// EnterSource_schema_name is called when entering the source_schema_name production.
	EnterSource_schema_name(c *Source_schema_nameContext)

	// EnterSource_package_name is called when entering the source_package_name production.
	EnterSource_package_name(c *Source_package_nameContext)

	// EnterSource_procedure_name is called when entering the source_procedure_name production.
	EnterSource_procedure_name(c *Source_procedure_nameContext)

	// EnterSql_parameter_name is called when entering the sql_parameter_name production.
	EnterSql_parameter_name(c *Sql_parameter_nameContext)

	// EnterSql_variable_name is called when entering the sql_variable_name production.
	EnterSql_variable_name(c *Sql_variable_nameContext)

	// EnterTransition_variable_name is called when entering the transition_variable_name production.
	EnterTransition_variable_name(c *Transition_variable_nameContext)

	// EnterSavepoint_name is called when entering the savepoint_name production.
	EnterSavepoint_name(c *Savepoint_nameContext)

	// EnterSpecific_name is called when entering the specific_name production.
	EnterSpecific_name(c *Specific_nameContext)

	// EnterSchema is called when entering the schema production.
	EnterSchema(c *SchemaContext)

	// EnterSchema_name is called when entering the schema_name production.
	EnterSchema_name(c *Schema_nameContext)

	// EnterSearch_method_name is called when entering the search_method_name production.
	EnterSearch_method_name(c *Search_method_nameContext)

	// EnterServer_name is called when entering the server_name production.
	EnterServer_name(c *Server_nameContext)

	// EnterServer_option_name is called when entering the server_option_name production.
	EnterServer_option_name(c *Server_option_nameContext)

	// EnterSession_authorization_name is called when entering the session_authorization_name production.
	EnterSession_authorization_name(c *Session_authorization_nameContext)

	// EnterComponent_name is called when entering the component_name production.
	EnterComponent_name(c *Component_nameContext)

	// EnterSec_label_comp_name is called when entering the sec_label_comp_name production.
	EnterSec_label_comp_name(c *Sec_label_comp_nameContext)

	// EnterSecurity_policy_name is called when entering the security_policy_name production.
	EnterSecurity_policy_name(c *Security_policy_nameContext)

	// EnterSecurity_label_name is called when entering the security_label_name production.
	EnterSecurity_label_name(c *Security_label_nameContext)

	// EnterSequence_name is called when entering the sequence_name production.
	EnterSequence_name(c *Sequence_nameContext)

	// EnterService_class_name is called when entering the service_class_name production.
	EnterService_class_name(c *Service_class_nameContext)

	// EnterService_superclass_name is called when entering the service_superclass_name production.
	EnterService_superclass_name(c *Service_superclass_nameContext)

	// EnterStoragegroup_name is called when entering the storagegroup_name production.
	EnterStoragegroup_name(c *Storagegroup_nameContext)

	// EnterSupertype_name is called when entering the supertype_name production.
	EnterSupertype_name(c *Supertype_nameContext)

	// EnterSuperview_name is called when entering the superview_name production.
	EnterSuperview_name(c *Superview_nameContext)

	// EnterService_subclass_name is called when entering the service_subclass_name production.
	EnterService_subclass_name(c *Service_subclass_nameContext)

	// EnterStatement_name is called when entering the statement_name production.
	EnterStatement_name(c *Statement_nameContext)

	// EnterTable_name is called when entering the table_name production.
	EnterTable_name(c *Table_nameContext)

	// EnterTablespace_name is called when entering the tablespace_name production.
	EnterTablespace_name(c *Tablespace_nameContext)

	// EnterTarget_identifier is called when entering the target_identifier production.
	EnterTarget_identifier(c *Target_identifierContext)

	// EnterThreshold_name is called when entering the threshold_name production.
	EnterThreshold_name(c *Threshold_nameContext)

	// EnterTrigger_name is called when entering the trigger_name production.
	EnterTrigger_name(c *Trigger_nameContext)

	// EnterContext_name is called when entering the context_name production.
	EnterContext_name(c *Context_nameContext)

	// EnterUsage_list_name is called when entering the usage_list_name production.
	EnterUsage_list_name(c *Usage_list_nameContext)

	// EnterType_name is called when entering the type_name production.
	EnterType_name(c *Type_nameContext)

	// EnterType_mapping_name is called when entering the type_mapping_name production.
	EnterType_mapping_name(c *Type_mapping_nameContext)

	// EnterTyped_table_name is called when entering the typed_table_name production.
	EnterTyped_table_name(c *Typed_table_nameContext)

	// EnterTyped_view_name is called when entering the typed_view_name production.
	EnterTyped_view_name(c *Typed_view_nameContext)

	// EnterUser_mapping_option_name is called when entering the user_mapping_option_name production.
	EnterUser_mapping_option_name(c *User_mapping_option_nameContext)

	// EnterView_name is called when entering the view_name production.
	EnterView_name(c *View_nameContext)

	// EnterVariable_name is called when entering the variable_name production.
	EnterVariable_name(c *Variable_nameContext)

	// EnterWork_action_set_name is called when entering the work_action_set_name production.
	EnterWork_action_set_name(c *Work_action_set_nameContext)

	// EnterWork_class_set_name is called when entering the work_class_set_name production.
	EnterWork_class_set_name(c *Work_class_set_nameContext)

	// EnterWorkload_name is called when entering the workload_name production.
	EnterWorkload_name(c *Workload_nameContext)

	// EnterWork_action_name is called when entering the work_action_name production.
	EnterWork_action_name(c *Work_action_nameContext)

	// EnterWork_class_name is called when entering the work_class_name production.
	EnterWork_class_name(c *Work_class_nameContext)

	// EnterWrapper_name is called when entering the wrapper_name production.
	EnterWrapper_name(c *Wrapper_nameContext)

	// EnterWrapper_option_name is called when entering the wrapper_option_name production.
	EnterWrapper_option_name(c *Wrapper_option_nameContext)

	// EnterXsrobject_name is called when entering the xsrobject_name production.
	EnterXsrobject_name(c *Xsrobject_nameContext)

	// EnterParameter_name is called when entering the parameter_name production.
	EnterParameter_name(c *Parameter_nameContext)

	// EnterCursor_variable_name is called when entering the cursor_variable_name production.
	EnterCursor_variable_name(c *Cursor_variable_nameContext)

	// EnterAlias_name is called when entering the alias_name production.
	EnterAlias_name(c *Alias_nameContext)

	// EnterDb_partition_group_name is called when entering the db_partition_group_name production.
	EnterDb_partition_group_name(c *Db_partition_group_nameContext)

	// EnterSource_index_name is called when entering the source_index_name production.
	EnterSource_index_name(c *Source_index_nameContext)

	// EnterSource_table_name is called when entering the source_table_name production.
	EnterSource_table_name(c *Source_table_nameContext)

	// EnterSource_storagegroup_name is called when entering the source_storagegroup_name production.
	EnterSource_storagegroup_name(c *Source_storagegroup_nameContext)

	// EnterTarget_storagegroup_name is called when entering the target_storagegroup_name production.
	EnterTarget_storagegroup_name(c *Target_storagegroup_nameContext)

	// EnterSource_tablespace_name is called when entering the source_tablespace_name production.
	EnterSource_tablespace_name(c *Source_tablespace_nameContext)

	// EnterTarget_tablespace_name is called when entering the target_tablespace_name production.
	EnterTarget_tablespace_name(c *Target_tablespace_nameContext)

	// EnterUnqualified_function_name is called when entering the unqualified_function_name production.
	EnterUnqualified_function_name(c *Unqualified_function_nameContext)

	// EnterUnqualified_procedure_name is called when entering the unqualified_procedure_name production.
	EnterUnqualified_procedure_name(c *Unqualified_procedure_nameContext)

	// EnterUnqualified_specific_name is called when entering the unqualified_specific_name production.
	EnterUnqualified_specific_name(c *Unqualified_specific_nameContext)

	// EnterPeriod_name is called when entering the period_name production.
	EnterPeriod_name(c *Period_nameContext)

	// EnterHistory_table_name is called when entering the history_table_name production.
	EnterHistory_table_name(c *History_table_nameContext)

	// EnterXml_schema_name is called when entering the xml_schema_name production.
	EnterXml_schema_name(c *Xml_schema_nameContext)

	// EnterTodo is called when entering the todo production.
	EnterTodo(c *TodoContext)

	// ExitDb2_file is called when exiting the db2_file production.
	ExitDb2_file(c *Db2_fileContext)

	// ExitBatch is called when exiting the batch production.
	ExitBatch(c *BatchContext)

	// ExitSql_statement is called when exiting the sql_statement production.
	ExitSql_statement(c *Sql_statementContext)

	// ExitSql_schema_statement is called when exiting the sql_schema_statement production.
	ExitSql_schema_statement(c *Sql_schema_statementContext)

	// ExitSql_data_change_statement is called when exiting the sql_data_change_statement production.
	ExitSql_data_change_statement(c *Sql_data_change_statementContext)

	// ExitSql_data_statement is called when exiting the sql_data_statement production.
	ExitSql_data_statement(c *Sql_data_statementContext)

	// ExitSql_transaction_statement is called when exiting the sql_transaction_statement production.
	ExitSql_transaction_statement(c *Sql_transaction_statementContext)

	// ExitSql_connection_statement is called when exiting the sql_connection_statement production.
	ExitSql_connection_statement(c *Sql_connection_statementContext)

	// ExitSql_dynamic_statement is called when exiting the sql_dynamic_statement production.
	ExitSql_dynamic_statement(c *Sql_dynamic_statementContext)

	// ExitSql_session_statement is called when exiting the sql_session_statement production.
	ExitSql_session_statement(c *Sql_session_statementContext)

	// ExitSql_embedded_host_language_statement is called when exiting the sql_embedded_host_language_statement production.
	ExitSql_embedded_host_language_statement(c *Sql_embedded_host_language_statementContext)

	// ExitSql_constrol_statement is called when exiting the sql_constrol_statement production.
	ExitSql_constrol_statement(c *Sql_constrol_statementContext)

	// ExitSelect_statement is called when exiting the select_statement production.
	ExitSelect_statement(c *Select_statementContext)

	// ExitRead_only_clause is called when exiting the read_only_clause production.
	ExitRead_only_clause(c *Read_only_clauseContext)

	// ExitUpdate_clause is called when exiting the update_clause production.
	ExitUpdate_clause(c *Update_clauseContext)

	// ExitOptimize_for_clause is called when exiting the optimize_for_clause production.
	ExitOptimize_for_clause(c *Optimize_for_clauseContext)

	// ExitConcurrent_access_resolution_clause is called when exiting the concurrent_access_resolution_clause production.
	ExitConcurrent_access_resolution_clause(c *Concurrent_access_resolution_clauseContext)

	// ExitDelete_statement is called when exiting the delete_statement production.
	ExitDelete_statement(c *Delete_statementContext)

	// ExitDelete_statement_searched_delete is called when exiting the delete_statement_searched_delete production.
	ExitDelete_statement_searched_delete(c *Delete_statement_searched_deleteContext)

	// ExitTable_or_view_name is called when exiting the table_or_view_name production.
	ExitTable_or_view_name(c *Table_or_view_nameContext)

	// ExitDelete_statement_positioned_delete is called when exiting the delete_statement_positioned_delete production.
	ExitDelete_statement_positioned_delete(c *Delete_statement_positioned_deleteContext)

	// ExitDelete_deltalake_statement is called when exiting the delete_deltalake_statement production.
	ExitDelete_deltalake_statement(c *Delete_deltalake_statementContext)

	// ExitInsert_statement is called when exiting the insert_statement production.
	ExitInsert_statement(c *Insert_statementContext)

	// ExitInsert_datalake_statement is called when exiting the insert_datalake_statement production.
	ExitInsert_datalake_statement(c *Insert_datalake_statementContext)

	// ExitValues_item is called when exiting the values_item production.
	ExitValues_item(c *Values_itemContext)

	// ExitMerge_statement is called when exiting the merge_statement production.
	ExitMerge_statement(c *Merge_statementContext)

	// ExitTable_view_fullselect is called when exiting the table_view_fullselect production.
	ExitTable_view_fullselect(c *Table_view_fullselectContext)

	// ExitCommon_table_expression_list is called when exiting the common_table_expression_list production.
	ExitCommon_table_expression_list(c *Common_table_expression_listContext)

	// ExitMatching_condition is called when exiting the matching_condition production.
	ExitMatching_condition(c *Matching_conditionContext)

	// ExitModification_operation is called when exiting the modification_operation production.
	ExitModification_operation(c *Modification_operationContext)

	// ExitUpdate_operation is called when exiting the update_operation production.
	ExitUpdate_operation(c *Update_operationContext)

	// ExitDelete_operation is called when exiting the delete_operation production.
	ExitDelete_operation(c *Delete_operationContext)

	// ExitInsert_operation is called when exiting the insert_operation production.
	ExitInsert_operation(c *Insert_operationContext)

	// ExitExpr_null_default_list is called when exiting the expr_null_default_list production.
	ExitExpr_null_default_list(c *Expr_null_default_listContext)

	// ExitIsolation_level is called when exiting the isolation_level production.
	ExitIsolation_level(c *Isolation_levelContext)

	// ExitTruncate_statement is called when exiting the truncate_statement production.
	ExitTruncate_statement(c *Truncate_statementContext)

	// ExitUpdate_statement is called when exiting the update_statement production.
	ExitUpdate_statement(c *Update_statementContext)

	// ExitUpdate_statement_searched_update is called when exiting the update_statement_searched_update production.
	ExitUpdate_statement_searched_update(c *Update_statement_searched_updateContext)

	// ExitSkip_wait is called when exiting the skip_wait production.
	ExitSkip_wait(c *Skip_waitContext)

	// ExitUpdate_statement_positioned_update is called when exiting the update_statement_positioned_update production.
	ExitUpdate_statement_positioned_update(c *Update_statement_positioned_updateContext)

	// ExitInclude_columns is called when exiting the include_columns production.
	ExitInclude_columns(c *Include_columnsContext)

	// ExitAssignment_clause is called when exiting the assignment_clause production.
	ExitAssignment_clause(c *Assignment_clauseContext)

	// ExitAssignment_item is called when exiting the assignment_item production.
	ExitAssignment_item(c *Assignment_itemContext)

	// ExitPeriod_clause is called when exiting the period_clause production.
	ExitPeriod_clause(c *Period_clauseContext)

	// ExitTime_sec is called when exiting the time_sec production.
	ExitTime_sec(c *Time_secContext)

	// ExitUpdate_datalake_statement is called when exiting the update_datalake_statement production.
	ExitUpdate_datalake_statement(c *Update_datalake_statementContext)

	// ExitGrant_database_authorities_statement is called when exiting the grant_database_authorities_statement production.
	ExitGrant_database_authorities_statement(c *Grant_database_authorities_statementContext)

	// ExitDb_privilege_list is called when exiting the db_privilege_list production.
	ExitDb_privilege_list(c *Db_privilege_listContext)

	// ExitDb_privilege is called when exiting the db_privilege production.
	ExitDb_privilege(c *Db_privilegeContext)

	// ExitGrantee is called when exiting the grantee production.
	ExitGrantee(c *GranteeContext)

	// ExitGrantee_user_group is called when exiting the grantee_user_group production.
	ExitGrantee_user_group(c *Grantee_user_groupContext)

	// ExitUser_group is called when exiting the user_group production.
	ExitUser_group(c *User_groupContext)

	// ExitGrantee_list is called when exiting the grantee_list production.
	ExitGrantee_list(c *Grantee_listContext)

	// ExitGrantee_list_public is called when exiting the grantee_list_public production.
	ExitGrantee_list_public(c *Grantee_list_publicContext)

	// ExitGrantee_list_user_group is called when exiting the grantee_list_user_group production.
	ExitGrantee_list_user_group(c *Grantee_list_user_groupContext)

	// ExitGrant_exemption_statement is called when exiting the grant_exemption_statement production.
	ExitGrant_exemption_statement(c *Grant_exemption_statementContext)

	// ExitExemption_privilege is called when exiting the exemption_privilege production.
	ExitExemption_privilege(c *Exemption_privilegeContext)

	// ExitGrant_global_variable_privileges_statement is called when exiting the grant_global_variable_privileges_statement production.
	ExitGrant_global_variable_privileges_statement(c *Grant_global_variable_privileges_statementContext)

	// ExitVariable_privilege is called when exiting the variable_privilege production.
	ExitVariable_privilege(c *Variable_privilegeContext)

	// ExitRead_write is called when exiting the read_write production.
	ExitRead_write(c *Read_writeContext)

	// ExitWith_grant_option is called when exiting the with_grant_option production.
	ExitWith_grant_option(c *With_grant_optionContext)

	// ExitGrant_index_privileges_statement is called when exiting the grant_index_privileges_statement production.
	ExitGrant_index_privileges_statement(c *Grant_index_privileges_statementContext)

	// ExitGrant_module_privileges_statement is called when exiting the grant_module_privileges_statement production.
	ExitGrant_module_privileges_statement(c *Grant_module_privileges_statementContext)

	// ExitGrant_package_privileges_statement is called when exiting the grant_package_privileges_statement production.
	ExitGrant_package_privileges_statement(c *Grant_package_privileges_statementContext)

	// ExitPackage_privilege_list is called when exiting the package_privilege_list production.
	ExitPackage_privilege_list(c *Package_privilege_listContext)

	// ExitPackage_privilege is called when exiting the package_privilege production.
	ExitPackage_privilege(c *Package_privilegeContext)

	// ExitGrant_role_statement is called when exiting the grant_role_statement production.
	ExitGrant_role_statement(c *Grant_role_statementContext)

	// ExitRole_list is called when exiting the role_list production.
	ExitRole_list(c *Role_listContext)

	// ExitGrant_routine_privileges_statement is called when exiting the grant_routine_privileges_statement production.
	ExitGrant_routine_privileges_statement(c *Grant_routine_privileges_statementContext)

	// ExitGrant_schema_privileges_statement is called when exiting the grant_schema_privileges_statement production.
	ExitGrant_schema_privileges_statement(c *Grant_schema_privileges_statementContext)

	// ExitSchema_privilege_list is called when exiting the schema_privilege_list production.
	ExitSchema_privilege_list(c *Schema_privilege_listContext)

	// ExitSchema_privilege is called when exiting the schema_privilege production.
	ExitSchema_privilege(c *Schema_privilegeContext)

	// ExitGrant_security_label_statement is called when exiting the grant_security_label_statement production.
	ExitGrant_security_label_statement(c *Grant_security_label_statementContext)

	// ExitGrant_sequence_privileges_statement is called when exiting the grant_sequence_privileges_statement production.
	ExitGrant_sequence_privileges_statement(c *Grant_sequence_privileges_statementContext)

	// ExitSequence_privilege_list is called when exiting the sequence_privilege_list production.
	ExitSequence_privilege_list(c *Sequence_privilege_listContext)

	// ExitSequence_privilege is called when exiting the sequence_privilege production.
	ExitSequence_privilege(c *Sequence_privilegeContext)

	// ExitGrant_server_privileges_statement is called when exiting the grant_server_privileges_statement production.
	ExitGrant_server_privileges_statement(c *Grant_server_privileges_statementContext)

	// ExitGrant_setsessionuser_privilege_statement is called when exiting the grant_setsessionuser_privilege_statement production.
	ExitGrant_setsessionuser_privilege_statement(c *Grant_setsessionuser_privilege_statementContext)

	// ExitUser_list is called when exiting the user_list production.
	ExitUser_list(c *User_listContext)

	// ExitUser_auth is called when exiting the user_auth production.
	ExitUser_auth(c *User_authContext)

	// ExitGrant_table_space_privileges_statement is called when exiting the grant_table_space_privileges_statement production.
	ExitGrant_table_space_privileges_statement(c *Grant_table_space_privileges_statementContext)

	// ExitGrant_table_view_or_nickname_privileges_statement is called when exiting the grant_table_view_or_nickname_privileges_statement production.
	ExitGrant_table_view_or_nickname_privileges_statement(c *Grant_table_view_or_nickname_privileges_statementContext)

	// ExitTvn_privilege_list is called when exiting the tvn_privilege_list production.
	ExitTvn_privilege_list(c *Tvn_privilege_listContext)

	// ExitTvn_privilege is called when exiting the tvn_privilege production.
	ExitTvn_privilege(c *Tvn_privilegeContext)

	// ExitColumn_name_list_paren is called when exiting the column_name_list_paren production.
	ExitColumn_name_list_paren(c *Column_name_list_parenContext)

	// ExitColumn_name_list is called when exiting the column_name_list production.
	ExitColumn_name_list(c *Column_name_listContext)

	// ExitGrant_workload_privileges_statement is called when exiting the grant_workload_privileges_statement production.
	ExitGrant_workload_privileges_statement(c *Grant_workload_privileges_statementContext)

	// ExitGrant_xsr_object_privileges_statement is called when exiting the grant_xsr_object_privileges_statement production.
	ExitGrant_xsr_object_privileges_statement(c *Grant_xsr_object_privileges_statementContext)

	// ExitRevoke_database_authorities_statement is called when exiting the revoke_database_authorities_statement production.
	ExitRevoke_database_authorities_statement(c *Revoke_database_authorities_statementContext)

	// ExitBy_all is called when exiting the by_all production.
	ExitBy_all(c *By_allContext)

	// ExitRevoke_exemption_statement is called when exiting the revoke_exemption_statement production.
	ExitRevoke_exemption_statement(c *Revoke_exemption_statementContext)

	// ExitRevoke_global_variable_privileges_statement is called when exiting the revoke_global_variable_privileges_statement production.
	ExitRevoke_global_variable_privileges_statement(c *Revoke_global_variable_privileges_statementContext)

	// ExitRevoke_index_privileges_statement is called when exiting the revoke_index_privileges_statement production.
	ExitRevoke_index_privileges_statement(c *Revoke_index_privileges_statementContext)

	// ExitRevoke_module_privileges_statement is called when exiting the revoke_module_privileges_statement production.
	ExitRevoke_module_privileges_statement(c *Revoke_module_privileges_statementContext)

	// ExitRevoke_package_privileges_statement is called when exiting the revoke_package_privileges_statement production.
	ExitRevoke_package_privileges_statement(c *Revoke_package_privileges_statementContext)

	// ExitRevoke_role_statement is called when exiting the revoke_role_statement production.
	ExitRevoke_role_statement(c *Revoke_role_statementContext)

	// ExitRevoke_routine_privileges_statement is called when exiting the revoke_routine_privileges_statement production.
	ExitRevoke_routine_privileges_statement(c *Revoke_routine_privileges_statementContext)

	// ExitRevoke_schema_privileges_statement is called when exiting the revoke_schema_privileges_statement production.
	ExitRevoke_schema_privileges_statement(c *Revoke_schema_privileges_statementContext)

	// ExitRevoke_security_label_statement is called when exiting the revoke_security_label_statement production.
	ExitRevoke_security_label_statement(c *Revoke_security_label_statementContext)

	// ExitRevoke_sequence_privileges_statement is called when exiting the revoke_sequence_privileges_statement production.
	ExitRevoke_sequence_privileges_statement(c *Revoke_sequence_privileges_statementContext)

	// ExitRevoke_server_privileges_statement is called when exiting the revoke_server_privileges_statement production.
	ExitRevoke_server_privileges_statement(c *Revoke_server_privileges_statementContext)

	// ExitRevoke_setsessionuser_privilege_statement is called when exiting the revoke_setsessionuser_privilege_statement production.
	ExitRevoke_setsessionuser_privilege_statement(c *Revoke_setsessionuser_privilege_statementContext)

	// ExitRevoke_table_space_privileges_statement is called when exiting the revoke_table_space_privileges_statement production.
	ExitRevoke_table_space_privileges_statement(c *Revoke_table_space_privileges_statementContext)

	// ExitRevoke_table_view_or_nickname_privileges_statement is called when exiting the revoke_table_view_or_nickname_privileges_statement production.
	ExitRevoke_table_view_or_nickname_privileges_statement(c *Revoke_table_view_or_nickname_privileges_statementContext)

	// ExitRevoke_workload_privileges_statement is called when exiting the revoke_workload_privileges_statement production.
	ExitRevoke_workload_privileges_statement(c *Revoke_workload_privileges_statementContext)

	// ExitRevoke_xsr_object_privileges_statement is called when exiting the revoke_xsr_object_privileges_statement production.
	ExitRevoke_xsr_object_privileges_statement(c *Revoke_xsr_object_privileges_statementContext)

	// ExitUser_group_role is called when exiting the user_group_role production.
	ExitUser_group_role(c *User_group_roleContext)

	// ExitRollback_statement is called when exiting the rollback_statement production.
	ExitRollback_statement(c *Rollback_statementContext)

	// ExitSavepoint_statement is called when exiting the savepoint_statement production.
	ExitSavepoint_statement(c *Savepoint_statementContext)

	// ExitRelease_savepoint_statement is called when exiting the release_savepoint_statement production.
	ExitRelease_savepoint_statement(c *Release_savepoint_statementContext)

	// ExitAllocate_cursor_statement is called when exiting the allocate_cursor_statement production.
	ExitAllocate_cursor_statement(c *Allocate_cursor_statementContext)

	// ExitAlter_audit_policy_statement is called when exiting the alter_audit_policy_statement production.
	ExitAlter_audit_policy_statement(c *Alter_audit_policy_statementContext)

	// ExitStatus_spec is called when exiting the status_spec production.
	ExitStatus_spec(c *Status_specContext)

	// ExitNormal_audit is called when exiting the normal_audit production.
	ExitNormal_audit(c *Normal_auditContext)

	// ExitAlter_bufferpool_statement is called when exiting the alter_bufferpool_statement production.
	ExitAlter_bufferpool_statement(c *Alter_bufferpool_statementContext)

	// ExitImmediate_deferred is called when exiting the immediate_deferred production.
	ExitImmediate_deferred(c *Immediate_deferredContext)

	// ExitAlter_database_partition_group_statement is called when exiting the alter_database_partition_group_statement production.
	ExitAlter_database_partition_group_statement(c *Alter_database_partition_group_statementContext)

	// ExitDb_partition_group_list_item is called when exiting the db_partition_group_list_item production.
	ExitDb_partition_group_list_item(c *Db_partition_group_list_itemContext)

	// ExitDb_partition_num_nums is called when exiting the db_partition_num_nums production.
	ExitDb_partition_num_nums(c *Db_partition_num_numsContext)

	// ExitDb_partitions_clause is called when exiting the db_partitions_clause production.
	ExitDb_partitions_clause(c *Db_partitions_clauseContext)

	// ExitDb_partition_options is called when exiting the db_partition_options production.
	ExitDb_partition_options(c *Db_partition_optionsContext)

	// ExitAlter_database_statement is called when exiting the alter_database_statement production.
	ExitAlter_database_statement(c *Alter_database_statementContext)

	// ExitAlter_database_opts is called when exiting the alter_database_opts production.
	ExitAlter_database_opts(c *Alter_database_optsContext)

	// ExitAlter_event_monitor_statement is called when exiting the alter_event_monitor_statement production.
	ExitAlter_event_monitor_statement(c *Alter_event_monitor_statementContext)

	// ExitAlter_event_monitor_opts is called when exiting the alter_event_monitor_opts production.
	ExitAlter_event_monitor_opts(c *Alter_event_monitor_optsContext)

	// ExitAlter_function_statement is called when exiting the alter_function_statement production.
	ExitAlter_function_statement(c *Alter_function_statementContext)

	// ExitAlter_function_opts is called when exiting the alter_function_opts production.
	ExitAlter_function_opts(c *Alter_function_optsContext)

	// ExitFunction_designator is called when exiting the function_designator production.
	ExitFunction_designator(c *Function_designatorContext)

	// ExitData_type_list is called when exiting the data_type_list production.
	ExitData_type_list(c *Data_type_listContext)

	// ExitData_type_list_paren is called when exiting the data_type_list_paren production.
	ExitData_type_list_paren(c *Data_type_list_parenContext)

	// ExitAlter_histogram_template_statement is called when exiting the alter_histogram_template_statement production.
	ExitAlter_histogram_template_statement(c *Alter_histogram_template_statementContext)

	// ExitAlter_index_statement is called when exiting the alter_index_statement production.
	ExitAlter_index_statement(c *Alter_index_statementContext)

	// ExitYes_no is called when exiting the yes_no production.
	ExitYes_no(c *Yes_noContext)

	// ExitAlter_mask_statement is called when exiting the alter_mask_statement production.
	ExitAlter_mask_statement(c *Alter_mask_statementContext)

	// ExitEnable_disable is called when exiting the enable_disable production.
	ExitEnable_disable(c *Enable_disableContext)

	// ExitAlter_method_statement is called when exiting the alter_method_statement production.
	ExitAlter_method_statement(c *Alter_method_statementContext)

	// ExitMethod_designator is called when exiting the method_designator production.
	ExitMethod_designator(c *Method_designatorContext)

	// ExitAlter_model_statement is called when exiting the alter_model_statement production.
	ExitAlter_model_statement(c *Alter_model_statementContext)

	// ExitAlter_module_statement is called when exiting the alter_module_statement production.
	ExitAlter_module_statement(c *Alter_module_statementContext)

	// ExitAlter_module_opts is called when exiting the alter_module_opts production.
	ExitAlter_module_opts(c *Alter_module_optsContext)

	// ExitModule_function_definition is called when exiting the module_function_definition production.
	ExitModule_function_definition(c *Module_function_definitionContext)

	// ExitModule_procedure_definition is called when exiting the module_procedure_definition production.
	ExitModule_procedure_definition(c *Module_procedure_definitionContext)

	// ExitModule_type_definition is called when exiting the module_type_definition production.
	ExitModule_type_definition(c *Module_type_definitionContext)

	// ExitModule_variable_definition is called when exiting the module_variable_definition production.
	ExitModule_variable_definition(c *Module_variable_definitionContext)

	// ExitModule_condition_definition is called when exiting the module_condition_definition production.
	ExitModule_condition_definition(c *Module_condition_definitionContext)

	// ExitModule_object_identification is called when exiting the module_object_identification production.
	ExitModule_object_identification(c *Module_object_identificationContext)

	// ExitModule_function_designator is called when exiting the module_function_designator production.
	ExitModule_function_designator(c *Module_function_designatorContext)

	// ExitModule_procedure_designator is called when exiting the module_procedure_designator production.
	ExitModule_procedure_designator(c *Module_procedure_designatorContext)

	// ExitAlter_nickname_statement is called when exiting the alter_nickname_statement production.
	ExitAlter_nickname_statement(c *Alter_nickname_statementContext)

	// ExitAlter_nickname_opts_1 is called when exiting the alter_nickname_opts_1 production.
	ExitAlter_nickname_opts_1(c *Alter_nickname_opts_1Context)

	// ExitAlter_nickname_opts_1_item is called when exiting the alter_nickname_opts_1_item production.
	ExitAlter_nickname_opts_1_item(c *Alter_nickname_opts_1_itemContext)

	// ExitAlter_nickname_opts_2 is called when exiting the alter_nickname_opts_2 production.
	ExitAlter_nickname_opts_2(c *Alter_nickname_opts_2Context)

	// ExitAlter_nickname_opts_2_item is called when exiting the alter_nickname_opts_2_item production.
	ExitAlter_nickname_opts_2_item(c *Alter_nickname_opts_2_itemContext)

	// ExitConstraint_alteration is called when exiting the constraint_alteration production.
	ExitConstraint_alteration(c *Constraint_alterationContext)

	// ExitAlter_package_statement is called when exiting the alter_package_statement production.
	ExitAlter_package_statement(c *Alter_package_statementContext)

	// ExitAlter_package_opts is called when exiting the alter_package_opts production.
	ExitAlter_package_opts(c *Alter_package_optsContext)

	// ExitAlter_permission_statement is called when exiting the alter_permission_statement production.
	ExitAlter_permission_statement(c *Alter_permission_statementContext)

	// ExitAlter_procedure_external_statement is called when exiting the alter_procedure_external_statement production.
	ExitAlter_procedure_external_statement(c *Alter_procedure_external_statementContext)

	// ExitAlter_procedure_external_opts is called when exiting the alter_procedure_external_opts production.
	ExitAlter_procedure_external_opts(c *Alter_procedure_external_optsContext)

	// ExitProcedure_designator is called when exiting the procedure_designator production.
	ExitProcedure_designator(c *Procedure_designatorContext)

	// ExitAlter_procedure_sourced_statement is called when exiting the alter_procedure_sourced_statement production.
	ExitAlter_procedure_sourced_statement(c *Alter_procedure_sourced_statementContext)

	// ExitParameter_alteration is called when exiting the parameter_alteration production.
	ExitParameter_alteration(c *Parameter_alterationContext)

	// ExitAlter_procedure_sql_statement is called when exiting the alter_procedure_sql_statement production.
	ExitAlter_procedure_sql_statement(c *Alter_procedure_sql_statementContext)

	// ExitAlter_schema_statement is called when exiting the alter_schema_statement production.
	ExitAlter_schema_statement(c *Alter_schema_statementContext)

	// ExitNone_changes is called when exiting the none_changes production.
	ExitNone_changes(c *None_changesContext)

	// ExitAlter_security_label_component_statement is called when exiting the alter_security_label_component_statement production.
	ExitAlter_security_label_component_statement(c *Alter_security_label_component_statementContext)

	// ExitAdd_element_clause is called when exiting the add_element_clause production.
	ExitAdd_element_clause(c *Add_element_clauseContext)

	// ExitArray_element_clause is called when exiting the array_element_clause production.
	ExitArray_element_clause(c *Array_element_clauseContext)

	// ExitTree_element_clause is called when exiting the tree_element_clause production.
	ExitTree_element_clause(c *Tree_element_clauseContext)

	// ExitAlter_security_policy_statement is called when exiting the alter_security_policy_statement production.
	ExitAlter_security_policy_statement(c *Alter_security_policy_statementContext)

	// ExitAlter_security_policy_opts is called when exiting the alter_security_policy_opts production.
	ExitAlter_security_policy_opts(c *Alter_security_policy_optsContext)

	// ExitAlter_sequence_statement is called when exiting the alter_sequence_statement production.
	ExitAlter_sequence_statement(c *Alter_sequence_statementContext)

	// ExitAlter_sequence_opts is called when exiting the alter_sequence_opts production.
	ExitAlter_sequence_opts(c *Alter_sequence_optsContext)

	// ExitAlter_server_statement is called when exiting the alter_server_statement production.
	ExitAlter_server_statement(c *Alter_server_statementContext)

	// ExitAlter_server_opts is called when exiting the alter_server_opts production.
	ExitAlter_server_opts(c *Alter_server_optsContext)

	// ExitAlter_service_class_statement is called when exiting the alter_service_class_statement production.
	ExitAlter_service_class_statement(c *Alter_service_class_statementContext)

	// ExitAlter_service_class_opts is called when exiting the alter_service_class_opts production.
	ExitAlter_service_class_opts(c *Alter_service_class_optsContext)

	// ExitDefault_on_off is called when exiting the default_on_off production.
	ExitDefault_on_off(c *Default_on_offContext)

	// ExitDefault_high_medium_low is called when exiting the default_high_medium_low production.
	ExitDefault_high_medium_low(c *Default_high_medium_lowContext)

	// ExitAlter_stogroup_statement is called when exiting the alter_stogroup_statement production.
	ExitAlter_stogroup_statement(c *Alter_stogroup_statementContext)

	// ExitAlter_stogroup_opts is called when exiting the alter_stogroup_opts production.
	ExitAlter_stogroup_opts(c *Alter_stogroup_optsContext)

	// ExitAlter_table_statement is called when exiting the alter_table_statement production.
	ExitAlter_table_statement(c *Alter_table_statementContext)

	// ExitAlter_table_opts is called when exiting the alter_table_opts production.
	ExitAlter_table_opts(c *Alter_table_optsContext)

	// ExitNull_on_off is called when exiting the null_on_off production.
	ExitNull_on_off(c *Null_on_offContext)

	// ExitCascade_restrict is called when exiting the cascade_restrict production.
	ExitCascade_restrict(c *Cascade_restrictContext)

	// ExitMaterialized_query_definition is called when exiting the materialized_query_definition production.
	ExitMaterialized_query_definition(c *Materialized_query_definitionContext)

	// ExitRefreshable_table_options is called when exiting the refreshable_table_options production.
	ExitRefreshable_table_options(c *Refreshable_table_optionsContext)

	// ExitColumn_alteration is called when exiting the column_alteration production.
	ExitColumn_alteration(c *Column_alterationContext)

	// ExitGeneration_alteration is called when exiting the generation_alteration production.
	ExitGeneration_alteration(c *Generation_alterationContext)

	// ExitIdentity_alteration is called when exiting the identity_alteration production.
	ExitIdentity_alteration(c *Identity_alterationContext)

	// ExitGeneration_attribute is called when exiting the generation_attribute production.
	ExitGeneration_attribute(c *Generation_attributeContext)

	// ExitAs_identity_clause is called when exiting the as_identity_clause production.
	ExitAs_identity_clause(c *As_identity_clauseContext)

	// ExitAs_identity_clause_opts is called when exiting the as_identity_clause_opts production.
	ExitAs_identity_clause_opts(c *As_identity_clause_optsContext)

	// ExitPeriod_definition_alter is called when exiting the period_definition_alter production.
	ExitPeriod_definition_alter(c *Period_definition_alterContext)

	// ExitAdd_partition is called when exiting the add_partition production.
	ExitAdd_partition(c *Add_partitionContext)

	// ExitBoundary_spec_alter is called when exiting the boundary_spec_alter production.
	ExitBoundary_spec_alter(c *Boundary_spec_alterContext)

	// ExitAttach_partition is called when exiting the attach_partition production.
	ExitAttach_partition(c *Attach_partitionContext)

	// ExitActivate_deactivate is called when exiting the activate_deactivate production.
	ExitActivate_deactivate(c *Activate_deactivateContext)

	// ExitAlter_tablespace_statement is called when exiting the alter_tablespace_statement production.
	ExitAlter_tablespace_statement(c *Alter_tablespace_statementContext)

	// ExitAlter_tablespace_opts is called when exiting the alter_tablespace_opts production.
	ExitAlter_tablespace_opts(c *Alter_tablespace_optsContext)

	// ExitAdd_clause is called when exiting the add_clause production.
	ExitAdd_clause(c *Add_clauseContext)

	// ExitDb_container_clause is called when exiting the db_container_clause production.
	ExitDb_container_clause(c *Db_container_clauseContext)

	// ExitDb_container_clause_opts is called when exiting the db_container_clause_opts production.
	ExitDb_container_clause_opts(c *Db_container_clause_optsContext)

	// ExitDrop_container_clause is called when exiting the drop_container_clause production.
	ExitDrop_container_clause(c *Drop_container_clauseContext)

	// ExitFile_device is called when exiting the file_device production.
	ExitFile_device(c *File_deviceContext)

	// ExitAll_containers_clause is called when exiting the all_containers_clause production.
	ExitAll_containers_clause(c *All_containers_clauseContext)

	// ExitSystem_container_clause is called when exiting the system_container_clause production.
	ExitSystem_container_clause(c *System_container_clauseContext)

	// ExitStripeset is called when exiting the stripeset production.
	ExitStripeset(c *StripesetContext)

	// ExitKm is called when exiting the km production.
	ExitKm(c *KmContext)

	// ExitKmg_percent is called when exiting the kmg_percent production.
	ExitKmg_percent(c *Kmg_percentContext)

	// ExitAlter_threshold_statement is called when exiting the alter_threshold_statement production.
	ExitAlter_threshold_statement(c *Alter_threshold_statementContext)

	// ExitAlter_threshold_opts is called when exiting the alter_threshold_opts production.
	ExitAlter_threshold_opts(c *Alter_threshold_optsContext)

	// ExitAlter_threshold_predicate is called when exiting the alter_threshold_predicate production.
	ExitAlter_threshold_predicate(c *Alter_threshold_predicateContext)

	// ExitAlter_threshold_exceeded_actions is called when exiting the alter_threshold_exceeded_actions production.
	ExitAlter_threshold_exceeded_actions(c *Alter_threshold_exceeded_actionsContext)

	// ExitDt_units is called when exiting the dt_units production.
	ExitDt_units(c *Dt_unitsContext)

	// ExitDt_units_with_seconds is called when exiting the dt_units_with_seconds production.
	ExitDt_units_with_seconds(c *Dt_units_with_secondsContext)

	// ExitAlter_trigger_statement is called when exiting the alter_trigger_statement production.
	ExitAlter_trigger_statement(c *Alter_trigger_statementContext)

	// ExitAlter_trusted_context_statement is called when exiting the alter_trusted_context_statement production.
	ExitAlter_trusted_context_statement(c *Alter_trusted_context_statementContext)

	// ExitAlter_trusted_context_opts is called when exiting the alter_trusted_context_opts production.
	ExitAlter_trusted_context_opts(c *Alter_trusted_context_optsContext)

	// ExitAlter_trusted_context_opts_alter_opts is called when exiting the alter_trusted_context_opts_alter_opts production.
	ExitAlter_trusted_context_opts_alter_opts(c *Alter_trusted_context_opts_alter_optsContext)

	// ExitAddr_clause_encryption_val is called when exiting the addr_clause_encryption_val production.
	ExitAddr_clause_encryption_val(c *Addr_clause_encryption_valContext)

	// ExitAddress_clause is called when exiting the address_clause production.
	ExitAddress_clause(c *Address_clauseContext)

	// ExitUser_clause is called when exiting the user_clause production.
	ExitUser_clause(c *User_clauseContext)

	// ExitUse_for_opts is called when exiting the use_for_opts production.
	ExitUse_for_opts(c *Use_for_optsContext)

	// ExitUse_for_opts_2 is called when exiting the use_for_opts_2 production.
	ExitUse_for_opts_2(c *Use_for_opts_2Context)

	// ExitAlter_type_statement is called when exiting the alter_type_statement production.
	ExitAlter_type_statement(c *Alter_type_statementContext)

	// ExitAlter_type_opts is called when exiting the alter_type_opts production.
	ExitAlter_type_opts(c *Alter_type_optsContext)

	// ExitMethod_identifier is called when exiting the method_identifier production.
	ExitMethod_identifier(c *Method_identifierContext)

	// ExitMethod_options is called when exiting the method_options production.
	ExitMethod_options(c *Method_optionsContext)

	// ExitAlter_usage_list_statement is called when exiting the alter_usage_list_statement production.
	ExitAlter_usage_list_statement(c *Alter_usage_list_statementContext)

	// ExitAlter_usage_list_opts_item is called when exiting the alter_usage_list_opts_item production.
	ExitAlter_usage_list_opts_item(c *Alter_usage_list_opts_itemContext)

	// ExitAlter_user_mapping_statement is called when exiting the alter_user_mapping_statement production.
	ExitAlter_user_mapping_statement(c *Alter_user_mapping_statementContext)

	// ExitAlter_user_mapping_opts_item is called when exiting the alter_user_mapping_opts_item production.
	ExitAlter_user_mapping_opts_item(c *Alter_user_mapping_opts_itemContext)

	// ExitAdd_set is called when exiting the add_set production.
	ExitAdd_set(c *Add_setContext)

	// ExitAlter_view_statement is called when exiting the alter_view_statement production.
	ExitAlter_view_statement(c *Alter_view_statementContext)

	// ExitAlter_view_opts is called when exiting the alter_view_opts production.
	ExitAlter_view_opts(c *Alter_view_optsContext)

	// ExitAlter_work_action_set_statement is called when exiting the alter_work_action_set_statement production.
	ExitAlter_work_action_set_statement(c *Alter_work_action_set_statementContext)

	// ExitAlter_work_action_set_opts is called when exiting the alter_work_action_set_opts production.
	ExitAlter_work_action_set_opts(c *Alter_work_action_set_optsContext)

	// ExitWork_action_alteration is called when exiting the work_action_alteration production.
	ExitWork_action_alteration(c *Work_action_alterationContext)

	// ExitWork_action_alteration_opts is called when exiting the work_action_alteration_opts production.
	ExitWork_action_alteration_opts(c *Work_action_alteration_optsContext)

	// ExitAlter_action_types_clause is called when exiting the alter_action_types_clause production.
	ExitAlter_action_types_clause(c *Alter_action_types_clauseContext)

	// ExitThreshold_predicate_clause is called when exiting the threshold_predicate_clause production.
	ExitThreshold_predicate_clause(c *Threshold_predicate_clauseContext)

	// ExitAlter_work_class_set_statement is called when exiting the alter_work_class_set_statement production.
	ExitAlter_work_class_set_statement(c *Alter_work_class_set_statementContext)

	// ExitAlter_work_class_set_opts is called when exiting the alter_work_class_set_opts production.
	ExitAlter_work_class_set_opts(c *Alter_work_class_set_optsContext)

	// ExitWork_class_alteration is called when exiting the work_class_alteration production.
	ExitWork_class_alteration(c *Work_class_alterationContext)

	// ExitWork_class_alteration_opts is called when exiting the work_class_alteration_opts production.
	ExitWork_class_alteration_opts(c *Work_class_alteration_optsContext)

	// ExitFor_from_to_alter_clause is called when exiting the for_from_to_alter_clause production.
	ExitFor_from_to_alter_clause(c *For_from_to_alter_clauseContext)

	// ExitSchema_alter_clause is called when exiting the schema_alter_clause production.
	ExitSchema_alter_clause(c *Schema_alter_clauseContext)

	// ExitData_tag_alter_clause is called when exiting the data_tag_alter_clause production.
	ExitData_tag_alter_clause(c *Data_tag_alter_clauseContext)

	// ExitAlter_workload_statement is called when exiting the alter_workload_statement production.
	ExitAlter_workload_statement(c *Alter_workload_statementContext)

	// ExitAlter_workload_opts_item is called when exiting the alter_workload_opts_item production.
	ExitAlter_workload_opts_item(c *Alter_workload_opts_itemContext)

	// ExitPackage_executable is called when exiting the package_executable production.
	ExitPackage_executable(c *Package_executableContext)

	// ExitBase_none is called when exiting the base_none production.
	ExitBase_none(c *Base_noneContext)

	// ExitExtended_base_none is called when exiting the extended_base_none production.
	ExitExtended_base_none(c *Extended_base_noneContext)

	// ExitAlter_collect_activity_data_clause is called when exiting the alter_collect_activity_data_clause production.
	ExitAlter_collect_activity_data_clause(c *Alter_collect_activity_data_clauseContext)

	// ExitWith_opts is called when exiting the with_opts production.
	ExitWith_opts(c *With_optsContext)

	// ExitAlter_collect_history_clause is called when exiting the alter_collect_history_clause production.
	ExitAlter_collect_history_clause(c *Alter_collect_history_clauseContext)

	// ExitAlter_collect_lock_wait_data_clause is called when exiting the alter_collect_lock_wait_data_clause production.
	ExitAlter_collect_lock_wait_data_clause(c *Alter_collect_lock_wait_data_clauseContext)

	// ExitAlter_wrapper_statement is called when exiting the alter_wrapper_statement production.
	ExitAlter_wrapper_statement(c *Alter_wrapper_statementContext)

	// ExitAlter_wrapper_opts_item is called when exiting the alter_wrapper_opts_item production.
	ExitAlter_wrapper_opts_item(c *Alter_wrapper_opts_itemContext)

	// ExitAlter_xsrobject_statement is called when exiting the alter_xsrobject_statement production.
	ExitAlter_xsrobject_statement(c *Alter_xsrobject_statementContext)

	// ExitString is called when exiting the string production.
	ExitString(c *StringContext)

	// ExitString_constant is called when exiting the string_constant production.
	ExitString_constant(c *String_constantContext)

	// ExitNumeric_constant is called when exiting the numeric_constant production.
	ExitNumeric_constant(c *Numeric_constantContext)

	// ExitData_type is called when exiting the data_type production.
	ExitData_type(c *Data_typeContext)

	// ExitAnchored_data_type is called when exiting the anchored_data_type production.
	ExitAnchored_data_type(c *Anchored_data_typeContext)

	// ExitAnchored_non_row_data_type is called when exiting the anchored_non_row_data_type production.
	ExitAnchored_non_row_data_type(c *Anchored_non_row_data_typeContext)

	// ExitAnchored_row_data_type is called when exiting the anchored_row_data_type production.
	ExitAnchored_row_data_type(c *Anchored_row_data_typeContext)

	// ExitSource_data_type is called when exiting the source_data_type production.
	ExitSource_data_type(c *Source_data_typeContext)

	// ExitData_type_constrainst is called when exiting the data_type_constrainst production.
	ExitData_type_constrainst(c *Data_type_constrainstContext)

	// ExitCheck_condition is called when exiting the check_condition production.
	ExitCheck_condition(c *Check_conditionContext)

	// ExitData_type_2 is called when exiting the data_type_2 production.
	ExitData_type_2(c *Data_type_2Context)

	// ExitBuilt_in_type is called when exiting the built_in_type production.
	ExitBuilt_in_type(c *Built_in_typeContext)

	// ExitInteger_paren is called when exiting the integer_paren production.
	ExitInteger_paren(c *Integer_parenContext)

	// ExitInteger_kmg_paren is called when exiting the integer_kmg_paren production.
	ExitInteger_kmg_paren(c *Integer_kmg_parenContext)

	// ExitChar_character is called when exiting the char_character production.
	ExitChar_character(c *Char_characterContext)

	// ExitOctets_codeunits is called when exiting the octets_codeunits production.
	ExitOctets_codeunits(c *Octets_codeunitsContext)

	// ExitCodeunits is called when exiting the codeunits production.
	ExitCodeunits(c *CodeunitsContext)

	// ExitKmg is called when exiting the kmg production.
	ExitKmg(c *KmgContext)

	// ExitRs_locator_variable is called when exiting the rs_locator_variable production.
	ExitRs_locator_variable(c *Rs_locator_variableContext)

	// ExitInteger_constant_list is called when exiting the integer_constant_list production.
	ExitInteger_constant_list(c *Integer_constant_listContext)

	// ExitInteger_constant is called when exiting the integer_constant production.
	ExitInteger_constant(c *Integer_constantContext)

	// ExitInteger_value is called when exiting the integer_value production.
	ExitInteger_value(c *Integer_valueContext)

	// ExitPositive_integer is called when exiting the positive_integer production.
	ExitPositive_integer(c *Positive_integerContext)

	// ExitBigint_value is called when exiting the bigint_value production.
	ExitBigint_value(c *Bigint_valueContext)

	// ExitBigint_constant is called when exiting the bigint_constant production.
	ExitBigint_constant(c *Bigint_constantContext)

	// ExitMember_number is called when exiting the member_number production.
	ExitMember_number(c *Member_numberContext)

	// ExitVersion_id is called when exiting the version_id production.
	ExitVersion_id(c *Version_idContext)

	// ExitDrop_statement is called when exiting the drop_statement production.
	ExitDrop_statement(c *Drop_statementContext)

	// ExitAlias_designator is called when exiting the alias_designator production.
	ExitAlias_designator(c *Alias_designatorContext)

	// ExitService_class_designator is called when exiting the service_class_designator production.
	ExitService_class_designator(c *Service_class_designatorContext)

	// ExitTablespace_name_list is called when exiting the tablespace_name_list production.
	ExitTablespace_name_list(c *Tablespace_name_listContext)

	// ExitAssociate_locators_statement is called when exiting the associate_locators_statement production.
	ExitAssociate_locators_statement(c *Associate_locators_statementContext)

	// ExitAudit_statement is called when exiting the audit_statement production.
	ExitAudit_statement(c *Audit_statementContext)

	// ExitBegin_declare_section_statement is called when exiting the begin_declare_section_statement production.
	ExitBegin_declare_section_statement(c *Begin_declare_section_statementContext)

	// ExitCall_statement is called when exiting the call_statement production.
	ExitCall_statement(c *Call_statementContext)

	// ExitArg_list_paren is called when exiting the arg_list_paren production.
	ExitArg_list_paren(c *Arg_list_parenContext)

	// ExitArg_list is called when exiting the arg_list production.
	ExitArg_list(c *Arg_listContext)

	// ExitArgument is called when exiting the argument production.
	ExitArgument(c *ArgumentContext)

	// ExitCase_statement is called when exiting the case_statement production.
	ExitCase_statement(c *Case_statementContext)

	// ExitSearched_case_statement_when_clause is called when exiting the searched_case_statement_when_clause production.
	ExitSearched_case_statement_when_clause(c *Searched_case_statement_when_clauseContext)

	// ExitSimple_case_statement_when_clause is called when exiting the simple_case_statement_when_clause production.
	ExitSimple_case_statement_when_clause(c *Simple_case_statement_when_clauseContext)

	// ExitClose_statement is called when exiting the close_statement production.
	ExitClose_statement(c *Close_statementContext)

	// ExitComment_statement is called when exiting the comment_statement production.
	ExitComment_statement(c *Comment_statementContext)

	// ExitColumn_comment is called when exiting the column_comment production.
	ExitColumn_comment(c *Column_commentContext)

	// ExitComment_objects is called when exiting the comment_objects production.
	ExitComment_objects(c *Comment_objectsContext)

	// ExitCommit_statement is called when exiting the commit_statement production.
	ExitCommit_statement(c *Commit_statementContext)

	// ExitConnect_type_1_statement is called when exiting the connect_type_1_statement production.
	ExitConnect_type_1_statement(c *Connect_type_1_statementContext)

	// ExitAuthorization is called when exiting the authorization production.
	ExitAuthorization(c *AuthorizationContext)

	// ExitPasswords is called when exiting the passwords production.
	ExitPasswords(c *PasswordsContext)

	// ExitLock_block is called when exiting the lock_block production.
	ExitLock_block(c *Lock_blockContext)

	// ExitAccesstoken is called when exiting the accesstoken production.
	ExitAccesstoken(c *AccesstokenContext)

	// ExitToken is called when exiting the token production.
	ExitToken(c *TokenContext)

	// ExitApi_key is called when exiting the api_key production.
	ExitApi_key(c *Api_keyContext)

	// ExitToken_type is called when exiting the token_type production.
	ExitToken_type(c *Token_typeContext)

	// ExitDeclare_cursor_statement is called when exiting the declare_cursor_statement production.
	ExitDeclare_cursor_statement(c *Declare_cursor_statementContext)

	// ExitDeclare_global_temporary_table_statement is called when exiting the declare_global_temporary_table_statement production.
	ExitDeclare_global_temporary_table_statement(c *Declare_global_temporary_table_statementContext)

	// ExitDescribe_statement is called when exiting the describe_statement production.
	ExitDescribe_statement(c *Describe_statementContext)

	// ExitXquery_statement is called when exiting the xquery_statement production.
	ExitXquery_statement(c *Xquery_statementContext)

	// ExitDescribe_input_statement is called when exiting the describe_input_statement production.
	ExitDescribe_input_statement(c *Describe_input_statementContext)

	// ExitDescribe_output_statement is called when exiting the describe_output_statement production.
	ExitDescribe_output_statement(c *Describe_output_statementContext)

	// ExitDisconnect_statement is called when exiting the disconnect_statement production.
	ExitDisconnect_statement(c *Disconnect_statementContext)

	// ExitEnd_declare_section_statement is called when exiting the end_declare_section_statement production.
	ExitEnd_declare_section_statement(c *End_declare_section_statementContext)

	// ExitExecute_statement is called when exiting the execute_statement production.
	ExitExecute_statement(c *Execute_statementContext)

	// ExitHost_variable_expression is called when exiting the host_variable_expression production.
	ExitHost_variable_expression(c *Host_variable_expressionContext)

	// ExitAssignment_target is called when exiting the assignment_target production.
	ExitAssignment_target(c *Assignment_targetContext)

	// ExitExecute_immediate_statement is called when exiting the execute_immediate_statement production.
	ExitExecute_immediate_statement(c *Execute_immediate_statementContext)

	// ExitExplain_statement is called when exiting the explain_statement production.
	ExitExplain_statement(c *Explain_statementContext)

	// ExitExplainable_sql_statement is called when exiting the explainable_sql_statement production.
	ExitExplainable_sql_statement(c *Explainable_sql_statementContext)

	// ExitFetch_statement is called when exiting the fetch_statement production.
	ExitFetch_statement(c *Fetch_statementContext)

	// ExitFlush_bufferpools_statement is called when exiting the flush_bufferpools_statement production.
	ExitFlush_bufferpools_statement(c *Flush_bufferpools_statementContext)

	// ExitFlush_event_monitor_statement is called when exiting the flush_event_monitor_statement production.
	ExitFlush_event_monitor_statement(c *Flush_event_monitor_statementContext)

	// ExitFlush_federated_cache_statement is called when exiting the flush_federated_cache_statement production.
	ExitFlush_federated_cache_statement(c *Flush_federated_cache_statementContext)

	// ExitFlush_optimization_profile_cache_statement is called when exiting the flush_optimization_profile_cache_statement production.
	ExitFlush_optimization_profile_cache_statement(c *Flush_optimization_profile_cache_statementContext)

	// ExitFlush_package_cache_statement is called when exiting the flush_package_cache_statement production.
	ExitFlush_package_cache_statement(c *Flush_package_cache_statementContext)

	// ExitFlush_authentication_cache_statement is called when exiting the flush_authentication_cache_statement production.
	ExitFlush_authentication_cache_statement(c *Flush_authentication_cache_statementContext)

	// ExitFree_locator_statement is called when exiting the free_locator_statement production.
	ExitFree_locator_statement(c *Free_locator_statementContext)

	// ExitGet_diagnostics_statement is called when exiting the get_diagnostics_statement production.
	ExitGet_diagnostics_statement(c *Get_diagnostics_statementContext)

	// ExitStatement_information is called when exiting the statement_information production.
	ExitStatement_information(c *Statement_informationContext)

	// ExitCondition_information is called when exiting the condition_information production.
	ExitCondition_information(c *Condition_informationContext)

	// ExitCondition_var_assignment is called when exiting the condition_var_assignment production.
	ExitCondition_var_assignment(c *Condition_var_assignmentContext)

	// ExitLock_table_statement is called when exiting the lock_table_statement production.
	ExitLock_table_statement(c *Lock_table_statementContext)

	// ExitPipe_statement is called when exiting the pipe_statement production.
	ExitPipe_statement(c *Pipe_statementContext)

	// ExitRefresh_table_statement is called when exiting the refresh_table_statement production.
	ExitRefresh_table_statement(c *Refresh_table_statementContext)

	// ExitRelease_connection_statement is called when exiting the release_connection_statement production.
	ExitRelease_connection_statement(c *Release_connection_statementContext)

	// ExitRename_statement is called when exiting the rename_statement production.
	ExitRename_statement(c *Rename_statementContext)

	// ExitRename_stogroup_statement is called when exiting the rename_stogroup_statement production.
	ExitRename_stogroup_statement(c *Rename_stogroup_statementContext)

	// ExitRename_tablespace_statement is called when exiting the rename_tablespace_statement production.
	ExitRename_tablespace_statement(c *Rename_tablespace_statementContext)

	// ExitSet_statement is called when exiting the set_statement production.
	ExitSet_statement(c *Set_statementContext)

	// ExitAccess_mode_clause is called when exiting the access_mode_clause production.
	ExitAccess_mode_clause(c *Access_mode_clauseContext)

	// ExitCascade_clause is called when exiting the cascade_clause production.
	ExitCascade_clause(c *Cascade_clauseContext)

	// ExitTo_descendent_types is called when exiting the to_descendent_types production.
	ExitTo_descendent_types(c *To_descendent_typesContext)

	// ExitTable_type_list is called when exiting the table_type_list production.
	ExitTable_type_list(c *Table_type_listContext)

	// ExitTable_type is called when exiting the table_type production.
	ExitTable_type(c *Table_typeContext)

	// ExitTable_checked_options_list is called when exiting the table_checked_options_list production.
	ExitTable_checked_options_list(c *Table_checked_options_listContext)

	// ExitTable_checked_options is called when exiting the table_checked_options production.
	ExitTable_checked_options(c *Table_checked_optionsContext)

	// ExitOnline_options is called when exiting the online_options production.
	ExitOnline_options(c *Online_optionsContext)

	// ExitQuery_optimization_options is called when exiting the query_optimization_options production.
	ExitQuery_optimization_options(c *Query_optimization_optionsContext)

	// ExitCheck_options is called when exiting the check_options production.
	ExitCheck_options(c *Check_optionsContext)

	// ExitIncremental_options is called when exiting the incremental_options production.
	ExitIncremental_options(c *Incremental_optionsContext)

	// ExitException_clause is called when exiting the exception_clause production.
	ExitException_clause(c *Exception_clauseContext)

	// ExitIn_table_use_clause is called when exiting the in_table_use_clause production.
	ExitIn_table_use_clause(c *In_table_use_clauseContext)

	// ExitTable_unchecked_options is called when exiting the table_unchecked_options production.
	ExitTable_unchecked_options(c *Table_unchecked_optionsContext)

	// ExitFull_access is called when exiting the full_access production.
	ExitFull_access(c *Full_accessContext)

	// ExitIntegrity_options is called when exiting the integrity_options production.
	ExitIntegrity_options(c *Integrity_optionsContext)

	// ExitIntegrity_options_item is called when exiting the integrity_options_item production.
	ExitIntegrity_options_item(c *Integrity_options_itemContext)

	// ExitVar_def_list is called when exiting the var_def_list production.
	ExitVar_def_list(c *Var_def_listContext)

	// ExitVar_def is called when exiting the var_def production.
	ExitVar_def(c *Var_defContext)

	// ExitExpr_null is called when exiting the expr_null production.
	ExitExpr_null(c *Expr_nullContext)

	// ExitExpr_null_default is called when exiting the expr_null_default production.
	ExitExpr_null_default(c *Expr_null_defaultContext)

	// ExitArray_index is called when exiting the array_index production.
	ExitArray_index(c *Array_indexContext)

	// ExitRow_fullselect is called when exiting the row_fullselect production.
	ExitRow_fullselect(c *Row_fullselectContext)

	// ExitTarget_variable is called when exiting the target_variable production.
	ExitTarget_variable(c *Target_variableContext)

	// ExitTarget_cursor_variable is called when exiting the target_cursor_variable production.
	ExitTarget_cursor_variable(c *Target_cursor_variableContext)

	// ExitTarget_row_variable is called when exiting the target_row_variable production.
	ExitTarget_row_variable(c *Target_row_variableContext)

	// ExitRow_array_element_specification is called when exiting the row_array_element_specification production.
	ExitRow_array_element_specification(c *Row_array_element_specificationContext)

	// ExitRow_field_reference is called when exiting the row_field_reference production.
	ExitRow_field_reference(c *Row_field_referenceContext)

	// ExitField_reference is called when exiting the field_reference production.
	ExitField_reference(c *Field_referenceContext)

	// ExitSearch_condition is called when exiting the search_condition production.
	ExitSearch_condition(c *Search_conditionContext)

	// ExitPredicate is called when exiting the predicate production.
	ExitPredicate(c *PredicateContext)

	// ExitAccording_to_clause is called when exiting the according_to_clause production.
	ExitAccording_to_clause(c *According_to_clauseContext)

	// ExitXml_schema_identification_list is called when exiting the xml_schema_identification_list production.
	ExitXml_schema_identification_list(c *Xml_schema_identification_listContext)

	// ExitXml_schema_identification is called when exiting the xml_schema_identification production.
	ExitXml_schema_identification(c *Xml_schema_identificationContext)

	// ExitFullselect_in_parentheses is called when exiting the fullselect_in_parentheses production.
	ExitFullselect_in_parentheses(c *Fullselect_in_parenthesesContext)

	// ExitSome_any_all is called when exiting the some_any_all production.
	ExitSome_any_all(c *Some_any_allContext)

	// ExitRow_value_expression is called when exiting the row_value_expression production.
	ExitRow_value_expression(c *Row_value_expressionContext)

	// ExitComparison_operator is called when exiting the comparison_operator production.
	ExitComparison_operator(c *Comparison_operatorContext)

	// ExitRow_expression is called when exiting the row_expression production.
	ExitRow_expression(c *Row_expressionContext)

	// ExitPath_opt_list is called when exiting the path_opt_list production.
	ExitPath_opt_list(c *Path_opt_listContext)

	// ExitPath_opt is called when exiting the path_opt production.
	ExitPath_opt(c *Path_optContext)

	// ExitPkg_opt_list is called when exiting the pkg_opt_list production.
	ExitPkg_opt_list(c *Pkg_opt_listContext)

	// ExitPkg_opt is called when exiting the pkg_opt production.
	ExitPkg_opt(c *Pkg_optContext)

	// ExitMaintain_opt_list is called when exiting the maintain_opt_list production.
	ExitMaintain_opt_list(c *Maintain_opt_listContext)

	// ExitMaintain_opt is called when exiting the maintain_opt production.
	ExitMaintain_opt(c *Maintain_optContext)

	// ExitVariable is called when exiting the variable production.
	ExitVariable(c *VariableContext)

	// ExitHost_variable is called when exiting the host_variable production.
	ExitHost_variable(c *Host_variableContext)

	// ExitSet_integrity_statement is called when exiting the set_integrity_statement production.
	ExitSet_integrity_statement(c *Set_integrity_statementContext)

	// ExitTransfer_ownership_statement is called when exiting the transfer_ownership_statement production.
	ExitTransfer_ownership_statement(c *Transfer_ownership_statementContext)

	// ExitObjects is called when exiting the objects production.
	ExitObjects(c *ObjectsContext)

	// ExitWhenever_statement is called when exiting the whenever_statement production.
	ExitWhenever_statement(c *Whenever_statementContext)

	// ExitFor_statement is called when exiting the for_statement production.
	ExitFor_statement(c *For_statementContext)

	// ExitGoto_statement is called when exiting the goto_statement production.
	ExitGoto_statement(c *Goto_statementContext)

	// ExitIf_statement is called when exiting the if_statement production.
	ExitIf_statement(c *If_statementContext)

	// ExitInclude_statement is called when exiting the include_statement production.
	ExitInclude_statement(c *Include_statementContext)

	// ExitResignal_statement is called when exiting the resignal_statement production.
	ExitResignal_statement(c *Resignal_statementContext)

	// ExitSignal_information is called when exiting the signal_information production.
	ExitSignal_information(c *Signal_informationContext)

	// ExitDiagnostic_string_constant is called when exiting the diagnostic_string_constant production.
	ExitDiagnostic_string_constant(c *Diagnostic_string_constantContext)

	// ExitSignal_statement is called when exiting the signal_statement production.
	ExitSignal_statement(c *Signal_statementContext)

	// ExitSqlstate_string_constant is called when exiting the sqlstate_string_constant production.
	ExitSqlstate_string_constant(c *Sqlstate_string_constantContext)

	// ExitSqlstate_string_variable is called when exiting the sqlstate_string_variable production.
	ExitSqlstate_string_variable(c *Sqlstate_string_variableContext)

	// ExitSignal_information_2 is called when exiting the signal_information_2 production.
	ExitSignal_information_2(c *Signal_information_2Context)

	// ExitDiagnostic_string_expression is called when exiting the diagnostic_string_expression production.
	ExitDiagnostic_string_expression(c *Diagnostic_string_expressionContext)

	// ExitIterate_statement is called when exiting the iterate_statement production.
	ExitIterate_statement(c *Iterate_statementContext)

	// ExitLeave_statement is called when exiting the leave_statement production.
	ExitLeave_statement(c *Leave_statementContext)

	// ExitLoop_statement is called when exiting the loop_statement production.
	ExitLoop_statement(c *Loop_statementContext)

	// ExitOpen_statement is called when exiting the open_statement production.
	ExitOpen_statement(c *Open_statementContext)

	// ExitVariable_or_expression is called when exiting the variable_or_expression production.
	ExitVariable_or_expression(c *Variable_or_expressionContext)

	// ExitSelect_into_statement is called when exiting the select_into_statement production.
	ExitSelect_into_statement(c *Select_into_statementContext)

	// ExitValues_into_statement is called when exiting the values_into_statement production.
	ExitValues_into_statement(c *Values_into_statementContext)

	// ExitPrepare_statement is called when exiting the prepare_statement production.
	ExitPrepare_statement(c *Prepare_statementContext)

	// ExitRepeat_statement is called when exiting the repeat_statement production.
	ExitRepeat_statement(c *Repeat_statementContext)

	// ExitReturn_statement is called when exiting the return_statement production.
	ExitReturn_statement(c *Return_statementContext)

	// ExitWhile_statement is called when exiting the while_statement production.
	ExitWhile_statement(c *While_statementContext)

	// ExitSql_routine_statement is called when exiting the sql_routine_statement production.
	ExitSql_routine_statement(c *Sql_routine_statementContext)

	// ExitCommon_table_expression is called when exiting the common_table_expression production.
	ExitCommon_table_expression(c *Common_table_expressionContext)

	// ExitCreate_alias_statement is called when exiting the create_alias_statement production.
	ExitCreate_alias_statement(c *Create_alias_statementContext)

	// ExitTable_alias is called when exiting the table_alias production.
	ExitTable_alias(c *Table_aliasContext)

	// ExitModule_alias is called when exiting the module_alias production.
	ExitModule_alias(c *Module_aliasContext)

	// ExitSequence_alias is called when exiting the sequence_alias production.
	ExitSequence_alias(c *Sequence_aliasContext)

	// ExitOr_replace is called when exiting the or_replace production.
	ExitOr_replace(c *Or_replaceContext)

	// ExitCreate_audit_policy_statement is called when exiting the create_audit_policy_statement production.
	ExitCreate_audit_policy_statement(c *Create_audit_policy_statementContext)

	// ExitAudit_policy_opts is called when exiting the audit_policy_opts production.
	ExitAudit_policy_opts(c *Audit_policy_optsContext)

	// ExitAudit_policy_categories_opts is called when exiting the audit_policy_categories_opts production.
	ExitAudit_policy_categories_opts(c *Audit_policy_categories_optsContext)

	// ExitCreate_bufferpool_statement is called when exiting the create_bufferpool_statement production.
	ExitCreate_bufferpool_statement(c *Create_bufferpool_statementContext)

	// ExitBufferpool_opts is called when exiting the bufferpool_opts production.
	ExitBufferpool_opts(c *Bufferpool_optsContext)

	// ExitExcept_clause is called when exiting the except_clause production.
	ExitExcept_clause(c *Except_clauseContext)

	// ExitMember_list is called when exiting the member_list production.
	ExitMember_list(c *Member_listContext)

	// ExitMember_list_item is called when exiting the member_list_item production.
	ExitMember_list_item(c *Member_list_itemContext)

	// ExitCreate_database_partition_group_statement is called when exiting the create_database_partition_group_statement production.
	ExitCreate_database_partition_group_statement(c *Create_database_partition_group_statementContext)

	// ExitCreate_event_monitor_statement is called when exiting the create_event_monitor_statement production.
	ExitCreate_event_monitor_statement(c *Create_event_monitor_statementContext)

	// ExitCreate_event_monitor_activities_statement is called when exiting the create_event_monitor_activities_statement production.
	ExitCreate_event_monitor_activities_statement(c *Create_event_monitor_activities_statementContext)

	// ExitFormatted_event_table_info_3 is called when exiting the formatted_event_table_info_3 production.
	ExitFormatted_event_table_info_3(c *Formatted_event_table_info_3Context)

	// ExitCreate_event_monitor_change_history_statement is called when exiting the create_event_monitor_change_history_statement production.
	ExitCreate_event_monitor_change_history_statement(c *Create_event_monitor_change_history_statementContext)

	// ExitEvent_control_list is called when exiting the event_control_list production.
	ExitEvent_control_list(c *Event_control_listContext)

	// ExitEvent_control is called when exiting the event_control production.
	ExitEvent_control(c *Event_controlContext)

	// ExitCreate_event_monitor_locking_statement is called when exiting the create_event_monitor_locking_statement production.
	ExitCreate_event_monitor_locking_statement(c *Create_event_monitor_locking_statementContext)

	// ExitCreate_event_monitor_package_cache_statement is called when exiting the create_event_monitor_package_cache_statement production.
	ExitCreate_event_monitor_package_cache_statement(c *Create_event_monitor_package_cache_statementContext)

	// ExitFilter_and_collection_options is called when exiting the filter_and_collection_options production.
	ExitFilter_and_collection_options(c *Filter_and_collection_optionsContext)

	// ExitEvent_condition is called when exiting the event_condition production.
	ExitEvent_condition(c *Event_conditionContext)

	// ExitEvent_condition_item is called when exiting the event_condition_item production.
	ExitEvent_condition_item(c *Event_condition_itemContext)

	// ExitCreate_event_monitor_statistics_statement is called when exiting the create_event_monitor_statistics_statement production.
	ExitCreate_event_monitor_statistics_statement(c *Create_event_monitor_statistics_statementContext)

	// ExitEvent_monitor_statistics_opts is called when exiting the event_monitor_statistics_opts production.
	ExitEvent_monitor_statistics_opts(c *Event_monitor_statistics_optsContext)

	// ExitCreate_event_monitor_threshold_violations_statement is called when exiting the create_event_monitor_threshold_violations_statement production.
	ExitCreate_event_monitor_threshold_violations_statement(c *Create_event_monitor_threshold_violations_statementContext)

	// ExitFormatted_event_table_info_2 is called when exiting the formatted_event_table_info_2 production.
	ExitFormatted_event_table_info_2(c *Formatted_event_table_info_2Context)

	// ExitFile_options is called when exiting the file_options production.
	ExitFile_options(c *File_optionsContext)

	// ExitEvent_monitor_threshold_opts is called when exiting the event_monitor_threshold_opts production.
	ExitEvent_monitor_threshold_opts(c *Event_monitor_threshold_optsContext)

	// ExitPages is called when exiting the pages production.
	ExitPages(c *PagesContext)

	// ExitCreate_event_monitor_unit_of_work is called when exiting the create_event_monitor_unit_of_work production.
	ExitCreate_event_monitor_unit_of_work(c *Create_event_monitor_unit_of_workContext)

	// ExitFormatted_event_table_info is called when exiting the formatted_event_table_info production.
	ExitFormatted_event_table_info(c *Formatted_event_table_infoContext)

	// ExitAutostart_manualstart is called when exiting the autostart_manualstart production.
	ExitAutostart_manualstart(c *Autostart_manualstartContext)

	// ExitEvm_group is called when exiting the evm_group production.
	ExitEvm_group(c *Evm_groupContext)

	// ExitTarget_table_options is called when exiting the target_table_options production.
	ExitTarget_table_options(c *Target_table_optionsContext)

	// ExitCreate_external_table_statement is called when exiting the create_external_table_statement production.
	ExitCreate_external_table_statement(c *Create_external_table_statementContext)

	// ExitExt_table_option is called when exiting the ext_table_option production.
	ExitExt_table_option(c *Ext_table_optionContext)

	// ExitExt_table_option_value is called when exiting the ext_table_option_value production.
	ExitExt_table_option_value(c *Ext_table_option_valueContext)

	// ExitCreate_function_statement is called when exiting the create_function_statement production.
	ExitCreate_function_statement(c *Create_function_statementContext)

	// ExitCreate_function_aggregate_interface_statement is called when exiting the create_function_aggregate_interface_statement production.
	ExitCreate_function_aggregate_interface_statement(c *Create_function_aggregate_interface_statementContext)

	// ExitAgg_fn_param_decl is called when exiting the agg_fn_param_decl production.
	ExitAgg_fn_param_decl(c *Agg_fn_param_declContext)

	// ExitAgg_fn_option_list is called when exiting the agg_fn_option_list production.
	ExitAgg_fn_option_list(c *Agg_fn_option_listContext)

	// ExitState_variable_declaration is called when exiting the state_variable_declaration production.
	ExitState_variable_declaration(c *State_variable_declarationContext)

	// ExitCreate_function_external_scalar_statement is called when exiting the create_function_external_scalar_statement production.
	ExitCreate_function_external_scalar_statement(c *Create_function_external_scalar_statementContext)

	// ExitExt_scalar_param_decl is called when exiting the ext_scalar_param_decl production.
	ExitExt_scalar_param_decl(c *Ext_scalar_param_declContext)

	// ExitExt_scalar_option_list is called when exiting the ext_scalar_option_list production.
	ExitExt_scalar_option_list(c *Ext_scalar_option_listContext)

	// ExitExt_scalar_option_list_item is called when exiting the ext_scalar_option_list_item production.
	ExitExt_scalar_option_list_item(c *Ext_scalar_option_list_itemContext)

	// ExitPredicate_specification is called when exiting the predicate_specification production.
	ExitPredicate_specification(c *Predicate_specificationContext)

	// ExitData_filter is called when exiting the data_filter production.
	ExitData_filter(c *Data_filterContext)

	// ExitIndex_exploitation is called when exiting the index_exploitation production.
	ExitIndex_exploitation(c *Index_exploitationContext)

	// ExitExploitation_rule is called when exiting the exploitation_rule production.
	ExitExploitation_rule(c *Exploitation_ruleContext)

	// ExitCreate_function_external_table_statement is called when exiting the create_function_external_table_statement production.
	ExitCreate_function_external_table_statement(c *Create_function_external_table_statementContext)

	// ExitExt_table_param_decl_list is called when exiting the ext_table_param_decl_list production.
	ExitExt_table_param_decl_list(c *Ext_table_param_decl_listContext)

	// ExitExt_table_param_decl is called when exiting the ext_table_param_decl production.
	ExitExt_table_param_decl(c *Ext_table_param_declContext)

	// ExitExt_table_option_list is called when exiting the ext_table_option_list production.
	ExitExt_table_option_list(c *Ext_table_option_listContext)

	// ExitExt_table_option_list_item is called when exiting the ext_table_option_list_item production.
	ExitExt_table_option_list_item(c *Ext_table_option_list_itemContext)

	// ExitCreate_function_old_db_external_function_statement is called when exiting the create_function_old_db_external_function_statement production.
	ExitCreate_function_old_db_external_function_statement(c *Create_function_old_db_external_function_statementContext)

	// ExitOledb_option_list is called when exiting the oledb_option_list production.
	ExitOledb_option_list(c *Oledb_option_listContext)

	// ExitOledb_option_list_item is called when exiting the oledb_option_list_item production.
	ExitOledb_option_list_item(c *Oledb_option_list_itemContext)

	// ExitCreate_function_sourced_or_template_statement is called when exiting the create_function_sourced_or_template_statement production.
	ExitCreate_function_sourced_or_template_statement(c *Create_function_sourced_or_template_statementContext)

	// ExitFn_return_opts is called when exiting the fn_return_opts production.
	ExitFn_return_opts(c *Fn_return_optsContext)

	// ExitFn_return_opts_item is called when exiting the fn_return_opts_item production.
	ExitFn_return_opts_item(c *Fn_return_opts_itemContext)

	// ExitTemplate_opts is called when exiting the template_opts production.
	ExitTemplate_opts(c *Template_optsContext)

	// ExitTemplate_opts_item is called when exiting the template_opts_item production.
	ExitTemplate_opts_item(c *Template_opts_itemContext)

	// ExitAscii_unicode is called when exiting the ascii_unicode production.
	ExitAscii_unicode(c *Ascii_unicodeContext)

	// ExitParam_decl_list_3 is called when exiting the param_decl_list_3 production.
	ExitParam_decl_list_3(c *Param_decl_list_3Context)

	// ExitParam_decl_3 is called when exiting the param_decl_3 production.
	ExitParam_decl_3(c *Param_decl_3Context)

	// ExitCreate_function_sql_scalar_table_or_row_statement is called when exiting the create_function_sql_scalar_table_or_row_statement production.
	ExitCreate_function_sql_scalar_table_or_row_statement(c *Create_function_sql_scalar_table_or_row_statementContext)

	// ExitParam_decl_list_2 is called when exiting the param_decl_list_2 production.
	ExitParam_decl_list_2(c *Param_decl_list_2Context)

	// ExitParam_decl_2 is called when exiting the param_decl_2 production.
	ExitParam_decl_2(c *Param_decl_2Context)

	// ExitSql_function_body is called when exiting the sql_function_body production.
	ExitSql_function_body(c *Sql_function_bodyContext)

	// ExitCreate_function_mapping_statement is called when exiting the create_function_mapping_statement production.
	ExitCreate_function_mapping_statement(c *Create_function_mapping_statementContext)

	// ExitFunction_options is called when exiting the function_options production.
	ExitFunction_options(c *Function_optionsContext)

	// ExitFunction_option_name is called when exiting the function_option_name production.
	ExitFunction_option_name(c *Function_option_nameContext)

	// ExitCreate_global_temporary_table_statement is called when exiting the create_global_temporary_table_statement production.
	ExitCreate_global_temporary_table_statement(c *Create_global_temporary_table_statementContext)

	// ExitCreate_global_temporary_table_opts is called when exiting the create_global_temporary_table_opts production.
	ExitCreate_global_temporary_table_opts(c *Create_global_temporary_table_optsContext)

	// ExitCreate_global_temporary_table_item is called when exiting the create_global_temporary_table_item production.
	ExitCreate_global_temporary_table_item(c *Create_global_temporary_table_itemContext)

	// ExitDelete_preserve is called when exiting the delete_preserve production.
	ExitDelete_preserve(c *Delete_preserveContext)

	// ExitCreate_histogram_template_statement is called when exiting the create_histogram_template_statement production.
	ExitCreate_histogram_template_statement(c *Create_histogram_template_statementContext)

	// ExitCreate_index_statement is called when exiting the create_index_statement production.
	ExitCreate_index_statement(c *Create_index_statementContext)

	// ExitIndex_col_opts is called when exiting the index_col_opts production.
	ExitIndex_col_opts(c *Index_col_optsContext)

	// ExitIndex_col_opts_item is called when exiting the index_col_opts_item production.
	ExitIndex_col_opts_item(c *Index_col_opts_itemContext)

	// ExitKey_expression is called when exiting the key_expression production.
	ExitKey_expression(c *Key_expressionContext)

	// ExitCreate_index_extension_statement is called when exiting the create_index_extension_statement production.
	ExitCreate_index_extension_statement(c *Create_index_extension_statementContext)

	// ExitParam_list is called when exiting the param_list production.
	ExitParam_list(c *Param_listContext)

	// ExitIndex_maintenance is called when exiting the index_maintenance production.
	ExitIndex_maintenance(c *Index_maintenanceContext)

	// ExitTable_function_invocation is called when exiting the table_function_invocation production.
	ExitTable_function_invocation(c *Table_function_invocationContext)

	// ExitIndex_search is called when exiting the index_search production.
	ExitIndex_search(c *Index_searchContext)

	// ExitSearch_method_definition is called when exiting the search_method_definition production.
	ExitSearch_method_definition(c *Search_method_definitionContext)

	// ExitCreate_mask_statement is called when exiting the create_mask_statement production.
	ExitCreate_mask_statement(c *Create_mask_statementContext)

	// ExitCase_expression is called when exiting the case_expression production.
	ExitCase_expression(c *Case_expressionContext)

	// ExitRange_producing_funciton_invocation is called when exiting the range_producing_funciton_invocation production.
	ExitRange_producing_funciton_invocation(c *Range_producing_funciton_invocationContext)

	// ExitIndex_filtering_function_invocation is called when exiting the index_filtering_function_invocation production.
	ExitIndex_filtering_function_invocation(c *Index_filtering_function_invocationContext)

	// ExitCreate_method_statement is called when exiting the create_method_statement production.
	ExitCreate_method_statement(c *Create_method_statementContext)

	// ExitMethod_opts is called when exiting the method_opts production.
	ExitMethod_opts(c *Method_optsContext)

	// ExitMethod_opts_item is called when exiting the method_opts_item production.
	ExitMethod_opts_item(c *Method_opts_itemContext)

	// ExitMethod_signature is called when exiting the method_signature production.
	ExitMethod_signature(c *Method_signatureContext)

	// ExitMethod_param_list is called when exiting the method_param_list production.
	ExitMethod_param_list(c *Method_param_listContext)

	// ExitData_type_3 is called when exiting the data_type_3 production.
	ExitData_type_3(c *Data_type_3Context)

	// ExitData_type_4 is called when exiting the data_type_4 production.
	ExitData_type_4(c *Data_type_4Context)

	// ExitSql_method_body is called when exiting the sql_method_body production.
	ExitSql_method_body(c *Sql_method_bodyContext)

	// ExitCompound_sql_inlined is called when exiting the compound_sql_inlined production.
	ExitCompound_sql_inlined(c *Compound_sql_inlinedContext)

	// ExitSql_statement_inlined is called when exiting the sql_statement_inlined production.
	ExitSql_statement_inlined(c *Sql_statement_inlinedContext)

	// ExitCompound_sql_compiled is called when exiting the compound_sql_compiled production.
	ExitCompound_sql_compiled(c *Compound_sql_compiledContext)

	// ExitSql_statement_compiled is called when exiting the sql_statement_compiled production.
	ExitSql_statement_compiled(c *Sql_statement_compiledContext)

	// ExitCreate_module_statement is called when exiting the create_module_statement production.
	ExitCreate_module_statement(c *Create_module_statementContext)

	// ExitCreate_nickname_statement is called when exiting the create_nickname_statement production.
	ExitCreate_nickname_statement(c *Create_nickname_statementContext)

	// ExitNick_name_option_name is called when exiting the nick_name_option_name production.
	ExitNick_name_option_name(c *Nick_name_option_nameContext)

	// ExitRemote_object_name is called when exiting the remote_object_name production.
	ExitRemote_object_name(c *Remote_object_nameContext)

	// ExitNon_relational_data_definition is called when exiting the non_relational_data_definition production.
	ExitNon_relational_data_definition(c *Non_relational_data_definitionContext)

	// ExitNick_name_column_list is called when exiting the nick_name_column_list production.
	ExitNick_name_column_list(c *Nick_name_column_listContext)

	// ExitNick_name_column_list_item is called when exiting the nick_name_column_list_item production.
	ExitNick_name_column_list_item(c *Nick_name_column_list_itemContext)

	// ExitNick_name_column_definition is called when exiting the nick_name_column_definition production.
	ExitNick_name_column_definition(c *Nick_name_column_definitionContext)

	// ExitNick_name_column_options is called when exiting the nick_name_column_options production.
	ExitNick_name_column_options(c *Nick_name_column_optionsContext)

	// ExitFederated_column_options is called when exiting the federated_column_options production.
	ExitFederated_column_options(c *Federated_column_optionsContext)

	// ExitColumn_option_name is called when exiting the column_option_name production.
	ExitColumn_option_name(c *Column_option_nameContext)

	// ExitCreate_permission_statement is called when exiting the create_permission_statement production.
	ExitCreate_permission_statement(c *Create_permission_statementContext)

	// ExitCreate_procedure_statement is called when exiting the create_procedure_statement production.
	ExitCreate_procedure_statement(c *Create_procedure_statementContext)

	// ExitCreate_procedure_external_statement is called when exiting the create_procedure_external_statement production.
	ExitCreate_procedure_external_statement(c *Create_procedure_external_statementContext)

	// ExitProc_ext_param_list is called when exiting the proc_ext_param_list production.
	ExitProc_ext_param_list(c *Proc_ext_param_listContext)

	// ExitProc_ext_param is called when exiting the proc_ext_param production.
	ExitProc_ext_param(c *Proc_ext_paramContext)

	// ExitOption_list_2 is called when exiting the option_list_2 production.
	ExitOption_list_2(c *Option_list_2Context)

	// ExitOption_list_2_item is called when exiting the option_list_2_item production.
	ExitOption_list_2_item(c *Option_list_2_itemContext)

	// ExitCreate_procedure_sourced_statement is called when exiting the create_procedure_sourced_statement production.
	ExitCreate_procedure_sourced_statement(c *Create_procedure_sourced_statementContext)

	// ExitSource_procedure_clause is called when exiting the source_procedure_clause production.
	ExitSource_procedure_clause(c *Source_procedure_clauseContext)

	// ExitSource_object_name is called when exiting the source_object_name production.
	ExitSource_object_name(c *Source_object_nameContext)

	// ExitOption_list_1 is called when exiting the option_list_1 production.
	ExitOption_list_1(c *Option_list_1Context)

	// ExitOption_list_1_item is called when exiting the option_list_1_item production.
	ExitOption_list_1_item(c *Option_list_1_itemContext)

	// ExitResult_set_element_number is called when exiting the result_set_element_number production.
	ExitResult_set_element_number(c *Result_set_element_numberContext)

	// ExitUnique_id is called when exiting the unique_id production.
	ExitUnique_id(c *Unique_idContext)

	// ExitCreate_procedure_sql_statement is called when exiting the create_procedure_sql_statement production.
	ExitCreate_procedure_sql_statement(c *Create_procedure_sql_statementContext)

	// ExitProc_parameter_list is called when exiting the proc_parameter_list production.
	ExitProc_parameter_list(c *Proc_parameter_listContext)

	// ExitProc_parameter_list_item is called when exiting the proc_parameter_list_item production.
	ExitProc_parameter_list_item(c *Proc_parameter_list_itemContext)

	// ExitIn_out_inout is called when exiting the in_out_inout production.
	ExitIn_out_inout(c *In_out_inoutContext)

	// ExitOption_list is called when exiting the option_list production.
	ExitOption_list(c *Option_listContext)

	// ExitOption_list_item is called when exiting the option_list_item production.
	ExitOption_list_item(c *Option_list_itemContext)

	// ExitSql_procedure_body is called when exiting the sql_procedure_body production.
	ExitSql_procedure_body(c *Sql_procedure_bodyContext)

	// ExitCreate_role_statement is called when exiting the create_role_statement production.
	ExitCreate_role_statement(c *Create_role_statementContext)

	// ExitCreate_schema_statement is called when exiting the create_schema_statement production.
	ExitCreate_schema_statement(c *Create_schema_statementContext)

	// ExitSchema_sql_statement is called when exiting the schema_sql_statement production.
	ExitSchema_sql_statement(c *Schema_sql_statementContext)

	// ExitCreate_security_label_component_statement is called when exiting the create_security_label_component_statement production.
	ExitCreate_security_label_component_statement(c *Create_security_label_component_statementContext)

	// ExitArray_clause is called when exiting the array_clause production.
	ExitArray_clause(c *Array_clauseContext)

	// ExitSet_clause is called when exiting the set_clause production.
	ExitSet_clause(c *Set_clauseContext)

	// ExitTree_clause is called when exiting the tree_clause production.
	ExitTree_clause(c *Tree_clauseContext)

	// ExitTree_clause_item is called when exiting the tree_clause_item production.
	ExitTree_clause_item(c *Tree_clause_itemContext)

	// ExitCreate_security_label_statement is called when exiting the create_security_label_statement production.
	ExitCreate_security_label_statement(c *Create_security_label_statementContext)

	// ExitCreate_security_label_item is called when exiting the create_security_label_item production.
	ExitCreate_security_label_item(c *Create_security_label_itemContext)

	// ExitCreate_security_policy_statement is called when exiting the create_security_policy_statement production.
	ExitCreate_security_policy_statement(c *Create_security_policy_statementContext)

	// ExitCreate_sequence_statement is called when exiting the create_sequence_statement production.
	ExitCreate_sequence_statement(c *Create_sequence_statementContext)

	// ExitCreate_sequence_opts is called when exiting the create_sequence_opts production.
	ExitCreate_sequence_opts(c *Create_sequence_optsContext)

	// ExitCreate_sequence_opts_item is called when exiting the create_sequence_opts_item production.
	ExitCreate_sequence_opts_item(c *Create_sequence_opts_itemContext)

	// ExitCreate_service_class_statement is called when exiting the create_service_class_statement production.
	ExitCreate_service_class_statement(c *Create_service_class_statementContext)

	// ExitHigh_medium_low is called when exiting the high_medium_low production.
	ExitHigh_medium_low(c *High_medium_lowContext)

	// ExitOn_off is called when exiting the on_off production.
	ExitOn_off(c *On_offContext)

	// ExitSoft_hard is called when exiting the soft_hard production.
	ExitSoft_hard(c *Soft_hardContext)

	// ExitCreate_server_statement is called when exiting the create_server_statement production.
	ExitCreate_server_statement(c *Create_server_statementContext)

	// ExitPassword_ is called when exiting the password_ production.
	ExitPassword_(c *Password_Context)

	// ExitCreate_stogroup_statement is called when exiting the create_stogroup_statement production.
	ExitCreate_stogroup_statement(c *Create_stogroup_statementContext)

	// ExitCreate_stogroup_opts is called when exiting the create_stogroup_opts production.
	ExitCreate_stogroup_opts(c *Create_stogroup_optsContext)

	// ExitCreate_synonym_statement is called when exiting the create_synonym_statement production.
	ExitCreate_synonym_statement(c *Create_synonym_statementContext)

	// ExitCreate_table_statement is called when exiting the create_table_statement production.
	ExitCreate_table_statement(c *Create_table_statementContext)

	// ExitCreate_table_opts is called when exiting the create_table_opts production.
	ExitCreate_table_opts(c *Create_table_optsContext)

	// ExitTable_option_list is called when exiting the table_option_list production.
	ExitTable_option_list(c *Table_option_listContext)

	// ExitTable_option_list_item is called when exiting the table_option_list_item production.
	ExitTable_option_list_item(c *Table_option_list_itemContext)

	// ExitTable_option_name is called when exiting the table_option_name production.
	ExitTable_option_name(c *Table_option_nameContext)

	// ExitElement_list is called when exiting the element_list production.
	ExitElement_list(c *Element_listContext)

	// ExitElement_list_item is called when exiting the element_list_item production.
	ExitElement_list_item(c *Element_list_itemContext)

	// ExitColumn_definition is called when exiting the column_definition production.
	ExitColumn_definition(c *Column_definitionContext)

	// ExitPeriod_definition is called when exiting the period_definition production.
	ExitPeriod_definition(c *Period_definitionContext)

	// ExitUnique_constraint is called when exiting the unique_constraint production.
	ExitUnique_constraint(c *Unique_constraintContext)

	// ExitReferential_constraint is called when exiting the referential_constraint production.
	ExitReferential_constraint(c *Referential_constraintContext)

	// ExitCheck_constraint is called when exiting the check_constraint production.
	ExitCheck_constraint(c *Check_constraintContext)

	// ExitColumn_options is called when exiting the column_options production.
	ExitColumn_options(c *Column_optionsContext)

	// ExitColumn_options_item is called when exiting the column_options_item production.
	ExitColumn_options_item(c *Column_options_itemContext)

	// ExitReferences_clause is called when exiting the references_clause production.
	ExitReferences_clause(c *References_clauseContext)

	// ExitRule_clause is called when exiting the rule_clause production.
	ExitRule_clause(c *Rule_clauseContext)

	// ExitConstraint_attributes is called when exiting the constraint_attributes production.
	ExitConstraint_attributes(c *Constraint_attributesContext)

	// ExitDefault_clause is called when exiting the default_clause production.
	ExitDefault_clause(c *Default_clauseContext)

	// ExitDefault_values is called when exiting the default_values production.
	ExitDefault_values(c *Default_valuesContext)

	// ExitGenerated_clause is called when exiting the generated_clause production.
	ExitGenerated_clause(c *Generated_clauseContext)

	// ExitDatetime_special_register is called when exiting the datetime_special_register production.
	ExitDatetime_special_register(c *Datetime_special_registerContext)

	// ExitUser_special_register is called when exiting the user_special_register production.
	ExitUser_special_register(c *User_special_registerContext)

	// ExitCast_function is called when exiting the cast_function production.
	ExitCast_function(c *Cast_functionContext)

	// ExitIdentity_options is called when exiting the identity_options production.
	ExitIdentity_options(c *Identity_optionsContext)

	// ExitIdentity_options_item is called when exiting the identity_options_item production.
	ExitIdentity_options_item(c *Identity_options_itemContext)

	// ExitAs_row_change_timestamp_clause is called when exiting the as_row_change_timestamp_clause production.
	ExitAs_row_change_timestamp_clause(c *As_row_change_timestamp_clauseContext)

	// ExitAs_generated_expression_clause is called when exiting the as_generated_expression_clause production.
	ExitAs_generated_expression_clause(c *As_generated_expression_clauseContext)

	// ExitGeneration_expression is called when exiting the generation_expression production.
	ExitGeneration_expression(c *Generation_expressionContext)

	// ExitAs_row_transaction_timestamp_clause is called when exiting the as_row_transaction_timestamp_clause production.
	ExitAs_row_transaction_timestamp_clause(c *As_row_transaction_timestamp_clauseContext)

	// ExitAs_row_transaction_start_id_clause is called when exiting the as_row_transaction_start_id_clause production.
	ExitAs_row_transaction_start_id_clause(c *As_row_transaction_start_id_clauseContext)

	// ExitOid_column_definition is called when exiting the oid_column_definition production.
	ExitOid_column_definition(c *Oid_column_definitionContext)

	// ExitRange_partition_spec is called when exiting the range_partition_spec production.
	ExitRange_partition_spec(c *Range_partition_specContext)

	// ExitPartition_expression_list is called when exiting the partition_expression_list production.
	ExitPartition_expression_list(c *Partition_expression_listContext)

	// ExitPartition_expression is called when exiting the partition_expression production.
	ExitPartition_expression(c *Partition_expressionContext)

	// ExitPartition_element_list is called when exiting the partition_element_list production.
	ExitPartition_element_list(c *Partition_element_listContext)

	// ExitPartition_element is called when exiting the partition_element production.
	ExitPartition_element(c *Partition_elementContext)

	// ExitBoundary_spec is called when exiting the boundary_spec production.
	ExitBoundary_spec(c *Boundary_specContext)

	// ExitPartition_tablespace_options is called when exiting the partition_tablespace_options production.
	ExitPartition_tablespace_options(c *Partition_tablespace_optionsContext)

	// ExitDuration_label is called when exiting the duration_label production.
	ExitDuration_label(c *Duration_labelContext)

	// ExitStarting_clause is called when exiting the starting_clause production.
	ExitStarting_clause(c *Starting_clauseContext)

	// ExitConst_min_max_list is called when exiting the const_min_max_list production.
	ExitConst_min_max_list(c *Const_min_max_listContext)

	// ExitConst_min_max is called when exiting the const_min_max production.
	ExitConst_min_max(c *Const_min_maxContext)

	// ExitEnding_clause is called when exiting the ending_clause production.
	ExitEnding_clause(c *Ending_clauseContext)

	// ExitTyped_table_options is called when exiting the typed_table_options production.
	ExitTyped_table_options(c *Typed_table_optionsContext)

	// ExitTyped_element_list is called when exiting the typed_element_list production.
	ExitTyped_element_list(c *Typed_element_listContext)

	// ExitTyped_element_list_item is called when exiting the typed_element_list_item production.
	ExitTyped_element_list_item(c *Typed_element_list_itemContext)

	// ExitAs_result_table is called when exiting the as_result_table production.
	ExitAs_result_table(c *As_result_tableContext)

	// ExitCopy_options is called when exiting the copy_options production.
	ExitCopy_options(c *Copy_optionsContext)

	// ExitMaterialized_query_options is called when exiting the materialized_query_options production.
	ExitMaterialized_query_options(c *Materialized_query_optionsContext)

	// ExitStaging_table_definition is called when exiting the staging_table_definition production.
	ExitStaging_table_definition(c *Staging_table_definitionContext)

	// ExitDimensions_clause is called when exiting the dimensions_clause production.
	ExitDimensions_clause(c *Dimensions_clauseContext)

	// ExitCol_names is called when exiting the col_names production.
	ExitCol_names(c *Col_namesContext)

	// ExitSequence_key_spec is called when exiting the sequence_key_spec production.
	ExitSequence_key_spec(c *Sequence_key_specContext)

	// ExitSequence_key_spec_list is called when exiting the sequence_key_spec_list production.
	ExitSequence_key_spec_list(c *Sequence_key_spec_listContext)

	// ExitSequence_key_spec_list_item is called when exiting the sequence_key_spec_list_item production.
	ExitSequence_key_spec_list_item(c *Sequence_key_spec_list_itemContext)

	// ExitTablespace_clauses is called when exiting the tablespace_clauses production.
	ExitTablespace_clauses(c *Tablespace_clausesContext)

	// ExitDistribution_clause is called when exiting the distribution_clause production.
	ExitDistribution_clause(c *Distribution_clauseContext)

	// ExitPartitioning_clause is called when exiting the partitioning_clause production.
	ExitPartitioning_clause(c *Partitioning_clauseContext)

	// ExitIf_not_exists is called when exiting the if_not_exists production.
	ExitIf_not_exists(c *If_not_existsContext)

	// ExitCreate_tablespace_statement is called when exiting the create_tablespace_statement production.
	ExitCreate_tablespace_statement(c *Create_tablespace_statementContext)

	// ExitStorage_group is called when exiting the storage_group production.
	ExitStorage_group(c *Storage_groupContext)

	// ExitSize_attributes is called when exiting the size_attributes production.
	ExitSize_attributes(c *Size_attributesContext)

	// ExitSystem_containers is called when exiting the system_containers production.
	ExitSystem_containers(c *System_containersContext)

	// ExitContainer_string_list is called when exiting the container_string_list production.
	ExitContainer_string_list(c *Container_string_listContext)

	// ExitDatabase_containers is called when exiting the database_containers production.
	ExitDatabase_containers(c *Database_containersContext)

	// ExitContainer_clause is called when exiting the container_clause production.
	ExitContainer_clause(c *Container_clauseContext)

	// ExitContainer_clause_list is called when exiting the container_clause_list production.
	ExitContainer_clause_list(c *Container_clause_listContext)

	// ExitContainer_clause_list_item is called when exiting the container_clause_list_item production.
	ExitContainer_clause_list_item(c *Container_clause_list_itemContext)

	// ExitOn_db_partitions_clause is called when exiting the on_db_partitions_clause production.
	ExitOn_db_partitions_clause(c *On_db_partitions_clauseContext)

	// ExitDb_partition_number_list is called when exiting the db_partition_number_list production.
	ExitDb_partition_number_list(c *Db_partition_number_listContext)

	// ExitDb_partition_number_list_item is called when exiting the db_partition_number_list_item production.
	ExitDb_partition_number_list_item(c *Db_partition_number_list_itemContext)

	// ExitDb_partition_number is called when exiting the db_partition_number production.
	ExitDb_partition_number(c *Db_partition_numberContext)

	// ExitNumber_of_pages is called when exiting the number_of_pages production.
	ExitNumber_of_pages(c *Number_of_pagesContext)

	// ExitNumber_of_files is called when exiting the number_of_files production.
	ExitNumber_of_files(c *Number_of_filesContext)

	// ExitNumber_of_milliseconds is called when exiting the number_of_milliseconds production.
	ExitNumber_of_milliseconds(c *Number_of_millisecondsContext)

	// ExitNumber_megabytes_per_second is called when exiting the number_megabytes_per_second production.
	ExitNumber_megabytes_per_second(c *Number_megabytes_per_secondContext)

	// ExitCreate_threshold_statement is called when exiting the create_threshold_statement production.
	ExitCreate_threshold_statement(c *Create_threshold_statementContext)

	// ExitThreshold_domain is called when exiting the threshold_domain production.
	ExitThreshold_domain(c *Threshold_domainContext)

	// ExitStatement_text is called when exiting the statement_text production.
	ExitStatement_text(c *Statement_textContext)

	// ExitExecutable_id is called when exiting the executable_id production.
	ExitExecutable_id(c *Executable_idContext)

	// ExitEnforcement_scope is called when exiting the enforcement_scope production.
	ExitEnforcement_scope(c *Enforcement_scopeContext)

	// ExitThreshold_predicate is called when exiting the threshold_predicate production.
	ExitThreshold_predicate(c *Threshold_predicateContext)

	// ExitChecking_every is called when exiting the checking_every production.
	ExitChecking_every(c *Checking_everyContext)

	// ExitHour_to_seconds is called when exiting the hour_to_seconds production.
	ExitHour_to_seconds(c *Hour_to_secondsContext)

	// ExitDay_to_minutes is called when exiting the day_to_minutes production.
	ExitDay_to_minutes(c *Day_to_minutesContext)

	// ExitDay_to_seconds is called when exiting the day_to_seconds production.
	ExitDay_to_seconds(c *Day_to_secondsContext)

	// ExitThreshold_exceeded_actions_2 is called when exiting the threshold_exceeded_actions_2 production.
	ExitThreshold_exceeded_actions_2(c *Threshold_exceeded_actions_2Context)

	// ExitDetails_section is called when exiting the details_section production.
	ExitDetails_section(c *Details_sectionContext)

	// ExitRemap_activity_action is called when exiting the remap_activity_action production.
	ExitRemap_activity_action(c *Remap_activity_actionContext)

	// ExitCreate_transform_statement is called when exiting the create_transform_statement production.
	ExitCreate_transform_statement(c *Create_transform_statementContext)

	// ExitTranform_list is called when exiting the tranform_list production.
	ExitTranform_list(c *Tranform_listContext)

	// ExitTranform_list_item is called when exiting the tranform_list_item production.
	ExitTranform_list_item(c *Tranform_list_itemContext)

	// ExitTransform_group_list is called when exiting the transform_group_list production.
	ExitTransform_group_list(c *Transform_group_listContext)

	// ExitTransform_group_list_item is called when exiting the transform_group_list_item production.
	ExitTransform_group_list_item(c *Transform_group_list_itemContext)

	// ExitCreate_trigger_statement is called when exiting the create_trigger_statement production.
	ExitCreate_trigger_statement(c *Create_trigger_statementContext)

	// ExitRef_list is called when exiting the ref_list production.
	ExitRef_list(c *Ref_listContext)

	// ExitRef_list_item is called when exiting the ref_list_item production.
	ExitRef_list_item(c *Ref_list_itemContext)

	// ExitOld_new is called when exiting the old_new production.
	ExitOld_new(c *Old_newContext)

	// ExitCorrelation_name is called when exiting the correlation_name production.
	ExitCorrelation_name(c *Correlation_nameContext)

	// ExitIdentifier is called when exiting the identifier production.
	ExitIdentifier(c *IdentifierContext)

	// ExitTrigger_event is called when exiting the trigger_event production.
	ExitTrigger_event(c *Trigger_eventContext)

	// ExitTriggered_action is called when exiting the triggered_action production.
	ExitTriggered_action(c *Triggered_actionContext)

	// ExitSql_procedure_statement is called when exiting the sql_procedure_statement production.
	ExitSql_procedure_statement(c *Sql_procedure_statementContext)

	// ExitSql_function_statement is called when exiting the sql_function_statement production.
	ExitSql_function_statement(c *Sql_function_statementContext)

	// ExitCreate_trusted_context_statement is called when exiting the create_trusted_context_statement production.
	ExitCreate_trusted_context_statement(c *Create_trusted_context_statementContext)

	// ExitAttr_list is called when exiting the attr_list production.
	ExitAttr_list(c *Attr_listContext)

	// ExitAttr_list_item is called when exiting the attr_list_item production.
	ExitAttr_list_item(c *Attr_list_itemContext)

	// ExitAuth_list is called when exiting the auth_list production.
	ExitAuth_list(c *Auth_listContext)

	// ExitAuth_list_item is called when exiting the auth_list_item production.
	ExitAuth_list_item(c *Auth_list_itemContext)

	// ExitAddress_value is called when exiting the address_value production.
	ExitAddress_value(c *Address_valueContext)

	// ExitEncryption_value is called when exiting the encryption_value production.
	ExitEncryption_value(c *Encryption_valueContext)

	// ExitCreate_type_statement is called when exiting the create_type_statement production.
	ExitCreate_type_statement(c *Create_type_statementContext)

	// ExitCreate_type_array_statement is called when exiting the create_type_array_statement production.
	ExitCreate_type_array_statement(c *Create_type_array_statementContext)

	// ExitCreate_type_cursor_statement is called when exiting the create_type_cursor_statement production.
	ExitCreate_type_cursor_statement(c *Create_type_cursor_statementContext)

	// ExitCreate_type_distinct_statement is called when exiting the create_type_distinct_statement production.
	ExitCreate_type_distinct_statement(c *Create_type_distinct_statementContext)

	// ExitCreate_type_row_statement is called when exiting the create_type_row_statement production.
	ExitCreate_type_row_statement(c *Create_type_row_statementContext)

	// ExitField_definition_list_paren is called when exiting the field_definition_list_paren production.
	ExitField_definition_list_paren(c *Field_definition_list_parenContext)

	// ExitField_definition_list is called when exiting the field_definition_list production.
	ExitField_definition_list(c *Field_definition_listContext)

	// ExitField_definition is called when exiting the field_definition production.
	ExitField_definition(c *Field_definitionContext)

	// ExitCreate_type_structured_statement is called when exiting the create_type_structured_statement production.
	ExitCreate_type_structured_statement(c *Create_type_structured_statementContext)

	// ExitStructured_type_seq is called when exiting the structured_type_seq production.
	ExitStructured_type_seq(c *Structured_type_seqContext)

	// ExitAttribute_definition_list_paren is called when exiting the attribute_definition_list_paren production.
	ExitAttribute_definition_list_paren(c *Attribute_definition_list_parenContext)

	// ExitAttribute_definition_list is called when exiting the attribute_definition_list production.
	ExitAttribute_definition_list(c *Attribute_definition_listContext)

	// ExitAttribute_definition is called when exiting the attribute_definition production.
	ExitAttribute_definition(c *Attribute_definitionContext)

	// ExitMethod_specification_list is called when exiting the method_specification_list production.
	ExitMethod_specification_list(c *Method_specification_listContext)

	// ExitMethod_specification is called when exiting the method_specification production.
	ExitMethod_specification(c *Method_specificationContext)

	// ExitMethod_specification_seq is called when exiting the method_specification_seq production.
	ExitMethod_specification_seq(c *Method_specification_seqContext)

	// ExitAs_locator is called when exiting the as_locator production.
	ExitAs_locator(c *As_locatorContext)

	// ExitParam_decl_list_paren is called when exiting the param_decl_list_paren production.
	ExitParam_decl_list_paren(c *Param_decl_list_parenContext)

	// ExitParam_decl_list is called when exiting the param_decl_list production.
	ExitParam_decl_list(c *Param_decl_listContext)

	// ExitParam_decl is called when exiting the param_decl production.
	ExitParam_decl(c *Param_declContext)

	// ExitSql_routine_characteristics is called when exiting the sql_routine_characteristics production.
	ExitSql_routine_characteristics(c *Sql_routine_characteristicsContext)

	// ExitExternal_routine_characteristics is called when exiting the external_routine_characteristics production.
	ExitExternal_routine_characteristics(c *External_routine_characteristicsContext)

	// ExitLength is called when exiting the length production.
	ExitLength(c *LengthContext)

	// ExitRep_type is called when exiting the rep_type production.
	ExitRep_type(c *Rep_typeContext)

	// ExitVarchars is called when exiting the varchars production.
	ExitVarchars(c *VarcharsContext)

	// ExitVarbinaries is called when exiting the varbinaries production.
	ExitVarbinaries(c *VarbinariesContext)

	// ExitFor_bit_data is called when exiting the for_bit_data production.
	ExitFor_bit_data(c *For_bit_dataContext)

	// ExitLob_options is called when exiting the lob_options production.
	ExitLob_options(c *Lob_optionsContext)

	// ExitCreate_type_mapping_statement is called when exiting the create_type_mapping_statement production.
	ExitCreate_type_mapping_statement(c *Create_type_mapping_statementContext)

	// ExitFor_bit_data_precision is called when exiting the for_bit_data_precision production.
	ExitFor_bit_data_precision(c *For_bit_data_precisionContext)

	// ExitPrecision is called when exiting the precision production.
	ExitPrecision(c *PrecisionContext)

	// ExitScale is called when exiting the scale production.
	ExitScale(c *ScaleContext)

	// ExitPrecision_scale_comp is called when exiting the precision_scale_comp production.
	ExitPrecision_scale_comp(c *Precision_scale_compContext)

	// ExitFrom_to is called when exiting the from_to production.
	ExitFrom_to(c *From_toContext)

	// ExitData_source_data_type is called when exiting the data_source_data_type production.
	ExitData_source_data_type(c *Data_source_data_typeContext)

	// ExitLocal_data_type is called when exiting the local_data_type production.
	ExitLocal_data_type(c *Local_data_typeContext)

	// ExitRemote_server is called when exiting the remote_server production.
	ExitRemote_server(c *Remote_serverContext)

	// ExitServer_version is called when exiting the server_version production.
	ExitServer_version(c *Server_versionContext)

	// ExitServer_type is called when exiting the server_type production.
	ExitServer_type(c *Server_typeContext)

	// ExitVersion is called when exiting the version production.
	ExitVersion(c *VersionContext)

	// ExitRelease is called when exiting the release production.
	ExitRelease(c *ReleaseContext)

	// ExitMod is called when exiting the mod production.
	ExitMod(c *ModContext)

	// ExitCreate_usage_list_statement is called when exiting the create_usage_list_statement production.
	ExitCreate_usage_list_statement(c *Create_usage_list_statementContext)

	// ExitCreate_user_mapping_statement is called when exiting the create_user_mapping_statement production.
	ExitCreate_user_mapping_statement(c *Create_user_mapping_statementContext)

	// ExitUser_mapping_options_paren is called when exiting the user_mapping_options_paren production.
	ExitUser_mapping_options_paren(c *User_mapping_options_parenContext)

	// ExitUser_mapping_options is called when exiting the user_mapping_options production.
	ExitUser_mapping_options(c *User_mapping_optionsContext)

	// ExitCreate_variable_statement is called when exiting the create_variable_statement production.
	ExitCreate_variable_statement(c *Create_variable_statementContext)

	// ExitConstant_ is called when exiting the constant_ production.
	ExitConstant_(c *Constant_Context)

	// ExitSpecial_register is called when exiting the special_register production.
	ExitSpecial_register(c *Special_registerContext)

	// ExitGlobal_variable is called when exiting the global_variable production.
	ExitGlobal_variable(c *Global_variableContext)

	// ExitData_type_1 is called when exiting the data_type_1 production.
	ExitData_type_1(c *Data_type_1Context)

	// ExitCursor_value_constructor is called when exiting the cursor_value_constructor production.
	ExitCursor_value_constructor(c *Cursor_value_constructorContext)

	// ExitAnchored_variable_data_type is called when exiting the anchored_variable_data_type production.
	ExitAnchored_variable_data_type(c *Anchored_variable_data_typeContext)

	// ExitHoldability is called when exiting the holdability production.
	ExitHoldability(c *HoldabilityContext)

	// ExitReturnability is called when exiting the returnability production.
	ExitReturnability(c *ReturnabilityContext)

	// ExitCreate_view_statement is called when exiting the create_view_statement production.
	ExitCreate_view_statement(c *Create_view_statementContext)

	// ExitCreate_view_seq is called when exiting the create_view_seq production.
	ExitCreate_view_seq(c *Create_view_seqContext)

	// ExitFullselect is called when exiting the fullselect production.
	ExitFullselect(c *FullselectContext)

	// ExitSubselect is called when exiting the subselect production.
	ExitSubselect(c *SubselectContext)

	// ExitSelect_clause is called when exiting the select_clause production.
	ExitSelect_clause(c *Select_clauseContext)

	// ExitSelect_clause_item is called when exiting the select_clause_item production.
	ExitSelect_clause_item(c *Select_clause_itemContext)

	// ExitFrom_clause is called when exiting the from_clause production.
	ExitFrom_clause(c *From_clauseContext)

	// ExitTable_reference is called when exiting the table_reference production.
	ExitTable_reference(c *Table_referenceContext)

	// ExitTable_reference_list is called when exiting the table_reference_list production.
	ExitTable_reference_list(c *Table_reference_listContext)

	// ExitSingles_table_reference is called when exiting the singles_table_reference production.
	ExitSingles_table_reference(c *Singles_table_referenceContext)

	// ExitPeriod_specification is called when exiting the period_specification production.
	ExitPeriod_specification(c *Period_specificationContext)

	// ExitValue is called when exiting the value production.
	ExitValue(c *ValueContext)

	// ExitCorrelation_clause is called when exiting the correlation_clause production.
	ExitCorrelation_clause(c *Correlation_clauseContext)

	// ExitTablesample_clause is called when exiting the tablesample_clause production.
	ExitTablesample_clause(c *Tablesample_clauseContext)

	// ExitNumeric_expression is called when exiting the numeric_expression production.
	ExitNumeric_expression(c *Numeric_expressionContext)

	// ExitSingle_view_reference is called when exiting the single_view_reference production.
	ExitSingle_view_reference(c *Single_view_referenceContext)

	// ExitSingle_nickname_reference is called when exiting the single_nickname_reference production.
	ExitSingle_nickname_reference(c *Single_nickname_referenceContext)

	// ExitOnly_table_reference is called when exiting the only_table_reference production.
	ExitOnly_table_reference(c *Only_table_referenceContext)

	// ExitOuter_table_reference is called when exiting the outer_table_reference production.
	ExitOuter_table_reference(c *Outer_table_referenceContext)

	// ExitAnalyze_table_reference is called when exiting the analyze_table_reference production.
	ExitAnalyze_table_reference(c *Analyze_table_referenceContext)

	// ExitImplementation_clause is called when exiting the implementation_clause production.
	ExitImplementation_clause(c *Implementation_clauseContext)

	// ExitNested_table_reference is called when exiting the nested_table_reference production.
	ExitNested_table_reference(c *Nested_table_referenceContext)

	// ExitContinue_handler is called when exiting the continue_handler production.
	ExitContinue_handler(c *Continue_handlerContext)

	// ExitSpecific_condition_value is called when exiting the specific_condition_value production.
	ExitSpecific_condition_value(c *Specific_condition_valueContext)

	// ExitData_change_table_reference is called when exiting the data_change_table_reference production.
	ExitData_change_table_reference(c *Data_change_table_referenceContext)

	// ExitSearched_update_statement is called when exiting the searched_update_statement production.
	ExitSearched_update_statement(c *Searched_update_statementContext)

	// ExitSearched_delete_statement is called when exiting the searched_delete_statement production.
	ExitSearched_delete_statement(c *Searched_delete_statementContext)

	// ExitFinal_new is called when exiting the final_new production.
	ExitFinal_new(c *Final_newContext)

	// ExitFinal_new_old is called when exiting the final_new_old production.
	ExitFinal_new_old(c *Final_new_oldContext)

	// ExitTable_function_reference is called when exiting the table_function_reference production.
	ExitTable_function_reference(c *Table_function_referenceContext)

	// ExitTable_udf_cardinality_clause is called when exiting the table_udf_cardinality_clause production.
	ExitTable_udf_cardinality_clause(c *Table_udf_cardinality_clauseContext)

	// ExitTyped_correlation_clause is called when exiting the typed_correlation_clause production.
	ExitTyped_correlation_clause(c *Typed_correlation_clauseContext)

	// ExitColumn_name_data_type is called when exiting the column_name_data_type production.
	ExitColumn_name_data_type(c *Column_name_data_typeContext)

	// ExitCollection_derived_table is called when exiting the collection_derived_table production.
	ExitCollection_derived_table(c *Collection_derived_tableContext)

	// ExitTable_function is called when exiting the table_function production.
	ExitTable_function(c *Table_functionContext)

	// ExitXmltable_expression is called when exiting the xmltable_expression production.
	ExitXmltable_expression(c *Xmltable_expressionContext)

	// ExitXmltable_function is called when exiting the xmltable_function production.
	ExitXmltable_function(c *Xmltable_functionContext)

	// ExitJoined_table is called when exiting the joined_table production.
	ExitJoined_table(c *Joined_tableContext)

	// ExitJoin_condition is called when exiting the join_condition production.
	ExitJoin_condition(c *Join_conditionContext)

	// ExitOuter is called when exiting the outer production.
	ExitOuter(c *OuterContext)

	// ExitExternal_table_reference is called when exiting the external_table_reference production.
	ExitExternal_table_reference(c *External_table_referenceContext)

	// ExitColumn_definition_2 is called when exiting the column_definition_2 production.
	ExitColumn_definition_2(c *Column_definition_2Context)

	// ExitFile_name is called when exiting the file_name production.
	ExitFile_name(c *File_nameContext)

	// ExitWhere_clause is called when exiting the where_clause production.
	ExitWhere_clause(c *Where_clauseContext)

	// ExitGroup_by_clause is called when exiting the group_by_clause production.
	ExitGroup_by_clause(c *Group_by_clauseContext)

	// ExitGroup_by_clause_opts is called when exiting the group_by_clause_opts production.
	ExitGroup_by_clause_opts(c *Group_by_clause_optsContext)

	// ExitGrouping_expression is called when exiting the grouping_expression production.
	ExitGrouping_expression(c *Grouping_expressionContext)

	// ExitGrouping_sets is called when exiting the grouping_sets production.
	ExitGrouping_sets(c *Grouping_setsContext)

	// ExitSuper_groups is called when exiting the super_groups production.
	ExitSuper_groups(c *Super_groupsContext)

	// ExitGrant_total is called when exiting the grant_total production.
	ExitGrant_total(c *Grant_totalContext)

	// ExitHaving_clause is called when exiting the having_clause production.
	ExitHaving_clause(c *Having_clauseContext)

	// ExitOrder_by_clause is called when exiting the order_by_clause production.
	ExitOrder_by_clause(c *Order_by_clauseContext)

	// ExitOrder_by_clause_opts is called when exiting the order_by_clause_opts production.
	ExitOrder_by_clause_opts(c *Order_by_clause_optsContext)

	// ExitTable_designator is called when exiting the table_designator production.
	ExitTable_designator(c *Table_designatorContext)

	// ExitAsc_desc is called when exiting the asc_desc production.
	ExitAsc_desc(c *Asc_descContext)

	// ExitFirst_last is called when exiting the first_last production.
	ExitFirst_last(c *First_lastContext)

	// ExitSort_key is called when exiting the sort_key production.
	ExitSort_key(c *Sort_keyContext)

	// ExitSimple_column_name is called when exiting the simple_column_name production.
	ExitSimple_column_name(c *Simple_column_nameContext)

	// ExitSimple_integer is called when exiting the simple_integer production.
	ExitSimple_integer(c *Simple_integerContext)

	// ExitSork_key_expression is called when exiting the sork_key_expression production.
	ExitSork_key_expression(c *Sork_key_expressionContext)

	// ExitOffset_clause is called when exiting the offset_clause production.
	ExitOffset_clause(c *Offset_clauseContext)

	// ExitOffset_row_count is called when exiting the offset_row_count production.
	ExitOffset_row_count(c *Offset_row_countContext)

	// ExitFetch_clause is called when exiting the fetch_clause production.
	ExitFetch_clause(c *Fetch_clauseContext)

	// ExitFetch_row_count is called when exiting the fetch_row_count production.
	ExitFetch_row_count(c *Fetch_row_countContext)

	// ExitRow_rows is called when exiting the row_rows production.
	ExitRow_rows(c *Row_rowsContext)

	// ExitIsolation_clause is called when exiting the isolation_clause production.
	ExitIsolation_clause(c *Isolation_clauseContext)

	// ExitLock_request_clause is called when exiting the lock_request_clause production.
	ExitLock_request_clause(c *Lock_request_clauseContext)

	// ExitValues_clause is called when exiting the values_clause production.
	ExitValues_clause(c *Values_clauseContext)

	// ExitValues_row is called when exiting the values_row production.
	ExitValues_row(c *Values_rowContext)

	// ExitRoot_view_definition is called when exiting the root_view_definition production.
	ExitRoot_view_definition(c *Root_view_definitionContext)

	// ExitSubview_definition is called when exiting the subview_definition production.
	ExitSubview_definition(c *Subview_definitionContext)

	// ExitOid_column is called when exiting the oid_column production.
	ExitOid_column(c *Oid_columnContext)

	// ExitWith_options is called when exiting the with_options production.
	ExitWith_options(c *With_optionsContext)

	// ExitWith_option_def is called when exiting the with_option_def production.
	ExitWith_option_def(c *With_option_defContext)

	// ExitWith_option_scope_def is called when exiting the with_option_scope_def production.
	ExitWith_option_scope_def(c *With_option_scope_defContext)

	// ExitUnder_clause is called when exiting the under_clause production.
	ExitUnder_clause(c *Under_clauseContext)

	// ExitCreate_work_action_set_statement is called when exiting the create_work_action_set_statement production.
	ExitCreate_work_action_set_statement(c *Create_work_action_set_statementContext)

	// ExitWork_action_definition_list_paren is called when exiting the work_action_definition_list_paren production.
	ExitWork_action_definition_list_paren(c *Work_action_definition_list_parenContext)

	// ExitWork_action_definition_list is called when exiting the work_action_definition_list production.
	ExitWork_action_definition_list(c *Work_action_definition_listContext)

	// ExitWork_action_definition is called when exiting the work_action_definition production.
	ExitWork_action_definition(c *Work_action_definitionContext)

	// ExitAction_types_clause is called when exiting the action_types_clause production.
	ExitAction_types_clause(c *Action_types_clauseContext)

	// ExitThreshold_types_clause is called when exiting the threshold_types_clause production.
	ExitThreshold_types_clause(c *Threshold_types_clauseContext)

	// ExitSecond_seconds is called when exiting the second_seconds production.
	ExitSecond_seconds(c *Second_secondsContext)

	// ExitHours_minutes is called when exiting the hours_minutes production.
	ExitHours_minutes(c *Hours_minutesContext)

	// ExitThreshold_exceeded_actions is called when exiting the threshold_exceeded_actions production.
	ExitThreshold_exceeded_actions(c *Threshold_exceeded_actionsContext)

	// ExitCollect_activity_data_clause is called when exiting the collect_activity_data_clause production.
	ExitCollect_activity_data_clause(c *Collect_activity_data_clauseContext)

	// ExitWith_without is called when exiting the with_without production.
	ExitWith_without(c *With_withoutContext)

	// ExitHistogram_templace_clause is called when exiting the histogram_templace_clause production.
	ExitHistogram_templace_clause(c *Histogram_templace_clauseContext)

	// ExitCreate_work_class_set_statement is called when exiting the create_work_class_set_statement production.
	ExitCreate_work_class_set_statement(c *Create_work_class_set_statementContext)

	// ExitWork_class_definition_list_paren is called when exiting the work_class_definition_list_paren production.
	ExitWork_class_definition_list_paren(c *Work_class_definition_list_parenContext)

	// ExitWork_class_definition_list is called when exiting the work_class_definition_list production.
	ExitWork_class_definition_list(c *Work_class_definition_listContext)

	// ExitWork_class_definition is called when exiting the work_class_definition production.
	ExitWork_class_definition(c *Work_class_definitionContext)

	// ExitWork_attributes is called when exiting the work_attributes production.
	ExitWork_attributes(c *Work_attributesContext)

	// ExitPosition_clause is called when exiting the position_clause production.
	ExitPosition_clause(c *Position_clauseContext)

	// ExitPosition_ is called when exiting the position_ production.
	ExitPosition_(c *Position_Context)

	// ExitFor_from_to_clause is called when exiting the for_from_to_clause production.
	ExitFor_from_to_clause(c *For_from_to_clauseContext)

	// ExitFrom_value is called when exiting the from_value production.
	ExitFrom_value(c *From_valueContext)

	// ExitTo_value is called when exiting the to_value production.
	ExitTo_value(c *To_valueContext)

	// ExitData_tag_clause is called when exiting the data_tag_clause production.
	ExitData_tag_clause(c *Data_tag_clauseContext)

	// ExitSchema_clause is called when exiting the schema_clause production.
	ExitSchema_clause(c *Schema_clauseContext)

	// ExitCreate_workload_statement is called when exiting the create_workload_statement production.
	ExitCreate_workload_statement(c *Create_workload_statementContext)

	// ExitPkg_exec_seq is called when exiting the pkg_exec_seq production.
	ExitPkg_exec_seq(c *Pkg_exec_seqContext)

	// ExitPosition_clause_2 is called when exiting the position_clause_2 production.
	ExitPosition_clause_2(c *Position_clause_2Context)

	// ExitConnection_attributes is called when exiting the connection_attributes production.
	ExitConnection_attributes(c *Connection_attributesContext)

	// ExitString_list is called when exiting the string_list production.
	ExitString_list(c *String_listContext)

	// ExitString_list_paren is called when exiting the string_list_paren production.
	ExitString_list_paren(c *String_list_parenContext)

	// ExitWorkload_attributes is called when exiting the workload_attributes production.
	ExitWorkload_attributes(c *Workload_attributesContext)

	// ExitDegree is called when exiting the degree production.
	ExitDegree(c *DegreeContext)

	// ExitAllow_disallow is called when exiting the allow_disallow production.
	ExitAllow_disallow(c *Allow_disallowContext)

	// ExitCollect_on_clause is called when exiting the collect_on_clause production.
	ExitCollect_on_clause(c *Collect_on_clauseContext)

	// ExitCollect_details_clause is called when exiting the collect_details_clause production.
	ExitCollect_details_clause(c *Collect_details_clauseContext)

	// ExitCollect_lock_wait_options is called when exiting the collect_lock_wait_options production.
	ExitCollect_lock_wait_options(c *Collect_lock_wait_optionsContext)

	// ExitWait_time is called when exiting the wait_time production.
	ExitWait_time(c *Wait_timeContext)

	// ExitCreate_wrapper_statement is called when exiting the create_wrapper_statement production.
	ExitCreate_wrapper_statement(c *Create_wrapper_statementContext)

	// ExitWrapper_option_list is called when exiting the wrapper_option_list production.
	ExitWrapper_option_list(c *Wrapper_option_listContext)

	// ExitWrapper_option is called when exiting the wrapper_option production.
	ExitWrapper_option(c *Wrapper_optionContext)

	// ExitExpression is called when exiting the expression production.
	ExitExpression(c *ExpressionContext)

	// ExitFunction_invocation is called when exiting the function_invocation production.
	ExitFunction_invocation(c *Function_invocationContext)

	// ExitAll_distinct is called when exiting the all_distinct production.
	ExitAll_distinct(c *All_distinctContext)

	// ExitScalar_fullselect is called when exiting the scalar_fullselect production.
	ExitScalar_fullselect(c *Scalar_fullselectContext)

	// ExitCast_specification is called when exiting the cast_specification production.
	ExitCast_specification(c *Cast_specificationContext)

	// ExitCursor_cast_specification is called when exiting the cursor_cast_specification production.
	ExitCursor_cast_specification(c *Cursor_cast_specificationContext)

	// ExitRow_cast_specification is called when exiting the row_cast_specification production.
	ExitRow_cast_specification(c *Row_cast_specificationContext)

	// ExitInterval_cast_specification is called when exiting the interval_cast_specification production.
	ExitInterval_cast_specification(c *Interval_cast_specificationContext)

	// ExitXmlcast_specification is called when exiting the xmlcast_specification production.
	ExitXmlcast_specification(c *Xmlcast_specificationContext)

	// ExitArray_element_specification is called when exiting the array_element_specification production.
	ExitArray_element_specification(c *Array_element_specificationContext)

	// ExitArray_constructor is called when exiting the array_constructor production.
	ExitArray_constructor(c *Array_constructorContext)

	// ExitMethod_invocation is called when exiting the method_invocation production.
	ExitMethod_invocation(c *Method_invocationContext)

	// ExitOlap_specification is called when exiting the olap_specification production.
	ExitOlap_specification(c *Olap_specificationContext)

	// ExitOrdered_olap_specification is called when exiting the ordered_olap_specification production.
	ExitOrdered_olap_specification(c *Ordered_olap_specificationContext)

	// ExitWindow_partition_clause is called when exiting the window_partition_clause production.
	ExitWindow_partition_clause(c *Window_partition_clauseContext)

	// ExitWindow_order_clause is called when exiting the window_order_clause production.
	ExitWindow_order_clause(c *Window_order_clauseContext)

	// ExitNumbering_specification is called when exiting the numbering_specification production.
	ExitNumbering_specification(c *Numbering_specificationContext)

	// ExitAggregation_specification is called when exiting the aggregation_specification production.
	ExitAggregation_specification(c *Aggregation_specificationContext)

	// ExitOlap_aggregate_function is called when exiting the olap_aggregate_function production.
	ExitOlap_aggregate_function(c *Olap_aggregate_functionContext)

	// ExitFirst_value_function is called when exiting the first_value_function production.
	ExitFirst_value_function(c *First_value_functionContext)

	// ExitLast_value_function is called when exiting the last_value_function production.
	ExitLast_value_function(c *Last_value_functionContext)

	// ExitNth_value_function is called when exiting the nth_value_function production.
	ExitNth_value_function(c *Nth_value_functionContext)

	// ExitRatio_to_report_function is called when exiting the ratio_to_report_function production.
	ExitRatio_to_report_function(c *Ratio_to_report_functionContext)

	// ExitIgnore_respect_nulls is called when exiting the ignore_respect_nulls production.
	ExitIgnore_respect_nulls(c *Ignore_respect_nullsContext)

	// ExitFrom_first_last is called when exiting the from_first_last production.
	ExitFrom_first_last(c *From_first_lastContext)

	// ExitWindow_aggregation_group_clause is called when exiting the window_aggregation_group_clause production.
	ExitWindow_aggregation_group_clause(c *Window_aggregation_group_clauseContext)

	// ExitGroup_start is called when exiting the group_start production.
	ExitGroup_start(c *Group_startContext)

	// ExitGroup_between is called when exiting the group_between production.
	ExitGroup_between(c *Group_betweenContext)

	// ExitGroup_bound1 is called when exiting the group_bound1 production.
	ExitGroup_bound1(c *Group_bound1Context)

	// ExitGroup_bound2 is called when exiting the group_bound2 production.
	ExitGroup_bound2(c *Group_bound2Context)

	// ExitGroup_end is called when exiting the group_end production.
	ExitGroup_end(c *Group_endContext)

	// ExitRow_change_expression is called when exiting the row_change_expression production.
	ExitRow_change_expression(c *Row_change_expressionContext)

	// ExitSequence_reference is called when exiting the sequence_reference production.
	ExitSequence_reference(c *Sequence_referenceContext)

	// ExitSubtype_treatment is called when exiting the subtype_treatment production.
	ExitSubtype_treatment(c *Subtype_treatmentContext)

	// ExitExpression_list is called when exiting the expression_list production.
	ExitExpression_list(c *Expression_listContext)

	// ExitExpression_list_in_parentheses is called when exiting the expression_list_in_parentheses production.
	ExitExpression_list_in_parentheses(c *Expression_list_in_parenthesesContext)

	// ExitId_ is called when exiting the id_ production.
	ExitId_(c *Id_Context)

	// ExitExposed_name is called when exiting the exposed_name production.
	ExitExposed_name(c *Exposed_nameContext)

	// ExitName is called when exiting the name production.
	ExitName(c *NameContext)

	// ExitLabel is called when exiting the label production.
	ExitLabel(c *LabelContext)

	// ExitHost_label is called when exiting the host_label production.
	ExitHost_label(c *Host_labelContext)

	// ExitLibrary_name is called when exiting the library_name production.
	ExitLibrary_name(c *Library_nameContext)

	// ExitArray_type_name is called when exiting the array_type_name production.
	ExitArray_type_name(c *Array_type_nameContext)

	// ExitAttribute_name is called when exiting the attribute_name production.
	ExitAttribute_name(c *Attribute_nameContext)

	// ExitRow_type_name is called when exiting the row_type_name production.
	ExitRow_type_name(c *Row_type_nameContext)

	// ExitAuthorization_name is called when exiting the authorization_name production.
	ExitAuthorization_name(c *Authorization_nameContext)

	// ExitBoolean_variable_name is called when exiting the boolean_variable_name production.
	ExitBoolean_variable_name(c *Boolean_variable_nameContext)

	// ExitArray_variable_name is called when exiting the array_variable_name production.
	ExitArray_variable_name(c *Array_variable_nameContext)

	// ExitColumn_name is called when exiting the column_name production.
	ExitColumn_name(c *Column_nameContext)

	// ExitConstraint_name is called when exiting the constraint_name production.
	ExitConstraint_name(c *Constraint_nameContext)

	// ExitDescriptor_name is called when exiting the descriptor_name production.
	ExitDescriptor_name(c *Descriptor_nameContext)

	// ExitDistinct_type_name is called when exiting the distinct_type_name production.
	ExitDistinct_type_name(c *Distinct_type_nameContext)

	// ExitCursor_name is called when exiting the cursor_name production.
	ExitCursor_name(c *Cursor_nameContext)

	// ExitCursor_type_name is called when exiting the cursor_type_name production.
	ExitCursor_type_name(c *Cursor_type_nameContext)

	// ExitCondition_name is called when exiting the condition_name production.
	ExitCondition_name(c *Condition_nameContext)

	// ExitData_source_name is called when exiting the data_source_name production.
	ExitData_source_name(c *Data_source_nameContext)

	// ExitExpression_name is called when exiting the expression_name production.
	ExitExpression_name(c *Expression_nameContext)

	// ExitGroup_name is called when exiting the group_name production.
	ExitGroup_name(c *Group_nameContext)

	// ExitPolicy_name is called when exiting the policy_name production.
	ExitPolicy_name(c *Policy_nameContext)

	// ExitBufferpool_name is called when exiting the bufferpool_name production.
	ExitBufferpool_name(c *Bufferpool_nameContext)

	// ExitDb_partition_name is called when exiting the db_partition_name production.
	ExitDb_partition_name(c *Db_partition_nameContext)

	// ExitDatabase_name is called when exiting the database_name production.
	ExitDatabase_name(c *Database_nameContext)

	// ExitEvent_monitor_name is called when exiting the event_monitor_name production.
	ExitEvent_monitor_name(c *Event_monitor_nameContext)

	// ExitField_name is called when exiting the field_name production.
	ExitField_name(c *Field_nameContext)

	// ExitFor_loop_name is called when exiting the for_loop_name production.
	ExitFor_loop_name(c *For_loop_nameContext)

	// ExitFunction_name is called when exiting the function_name production.
	ExitFunction_name(c *Function_nameContext)

	// ExitFunction_mapping_name is called when exiting the function_mapping_name production.
	ExitFunction_mapping_name(c *Function_mapping_nameContext)

	// ExitGlobal_variable_name is called when exiting the global_variable_name production.
	ExitGlobal_variable_name(c *Global_variable_nameContext)

	// ExitHierarchy_name is called when exiting the hierarchy_name production.
	ExitHierarchy_name(c *Hierarchy_nameContext)

	// ExitHost_variable_name is called when exiting the host_variable_name production.
	ExitHost_variable_name(c *Host_variable_nameContext)

	// ExitParameter_marker is called when exiting the parameter_marker production.
	ExitParameter_marker(c *Parameter_markerContext)

	// ExitTemplate_name is called when exiting the template_name production.
	ExitTemplate_name(c *Template_nameContext)

	// ExitIndex_name is called when exiting the index_name production.
	ExitIndex_name(c *Index_nameContext)

	// ExitIndex_extension_name is called when exiting the index_extension_name production.
	ExitIndex_extension_name(c *Index_extension_nameContext)

	// ExitInput_descriptor_name is called when exiting the input_descriptor_name production.
	ExitInput_descriptor_name(c *Input_descriptor_nameContext)

	// ExitMask_name is called when exiting the mask_name production.
	ExitMask_name(c *Mask_nameContext)

	// ExitMethod_name is called when exiting the method_name production.
	ExitMethod_name(c *Method_nameContext)

	// ExitModel_name is called when exiting the model_name production.
	ExitModel_name(c *Model_nameContext)

	// ExitModule_name is called when exiting the module_name production.
	ExitModule_name(c *Module_nameContext)

	// ExitNew_owner is called when exiting the new_owner production.
	ExitNew_owner(c *New_ownerContext)

	// ExitNick_name is called when exiting the nick_name production.
	ExitNick_name(c *Nick_nameContext)

	// ExitObject_name is called when exiting the object_name production.
	ExitObject_name(c *Object_nameContext)

	// ExitOid_column_name is called when exiting the oid_column_name production.
	ExitOid_column_name(c *Oid_column_nameContext)

	// ExitOptimization_profile_name is called when exiting the optimization_profile_name production.
	ExitOptimization_profile_name(c *Optimization_profile_nameContext)

	// ExitPackage_name is called when exiting the package_name production.
	ExitPackage_name(c *Package_nameContext)

	// ExitPartition_name is called when exiting the partition_name production.
	ExitPartition_name(c *Partition_nameContext)

	// ExitPath_name is called when exiting the path_name production.
	ExitPath_name(c *Path_nameContext)

	// ExitPermission_name is called when exiting the permission_name production.
	ExitPermission_name(c *Permission_nameContext)

	// ExitPipe_name is called when exiting the pipe_name production.
	ExitPipe_name(c *Pipe_nameContext)

	// ExitProcedure_name is called when exiting the procedure_name production.
	ExitProcedure_name(c *Procedure_nameContext)

	// ExitResult_descriptor_name is called when exiting the result_descriptor_name production.
	ExitResult_descriptor_name(c *Result_descriptor_nameContext)

	// ExitRole_name is called when exiting the role_name production.
	ExitRole_name(c *Role_nameContext)

	// ExitRoot_table_name is called when exiting the root_table_name production.
	ExitRoot_table_name(c *Root_table_nameContext)

	// ExitRoot_view_name is called when exiting the root_view_name production.
	ExitRoot_view_name(c *Root_view_nameContext)

	// ExitRow_variable_name is called when exiting the row_variable_name production.
	ExitRow_variable_name(c *Row_variable_nameContext)

	// ExitSource_schema_name is called when exiting the source_schema_name production.
	ExitSource_schema_name(c *Source_schema_nameContext)

	// ExitSource_package_name is called when exiting the source_package_name production.
	ExitSource_package_name(c *Source_package_nameContext)

	// ExitSource_procedure_name is called when exiting the source_procedure_name production.
	ExitSource_procedure_name(c *Source_procedure_nameContext)

	// ExitSql_parameter_name is called when exiting the sql_parameter_name production.
	ExitSql_parameter_name(c *Sql_parameter_nameContext)

	// ExitSql_variable_name is called when exiting the sql_variable_name production.
	ExitSql_variable_name(c *Sql_variable_nameContext)

	// ExitTransition_variable_name is called when exiting the transition_variable_name production.
	ExitTransition_variable_name(c *Transition_variable_nameContext)

	// ExitSavepoint_name is called when exiting the savepoint_name production.
	ExitSavepoint_name(c *Savepoint_nameContext)

	// ExitSpecific_name is called when exiting the specific_name production.
	ExitSpecific_name(c *Specific_nameContext)

	// ExitSchema is called when exiting the schema production.
	ExitSchema(c *SchemaContext)

	// ExitSchema_name is called when exiting the schema_name production.
	ExitSchema_name(c *Schema_nameContext)

	// ExitSearch_method_name is called when exiting the search_method_name production.
	ExitSearch_method_name(c *Search_method_nameContext)

	// ExitServer_name is called when exiting the server_name production.
	ExitServer_name(c *Server_nameContext)

	// ExitServer_option_name is called when exiting the server_option_name production.
	ExitServer_option_name(c *Server_option_nameContext)

	// ExitSession_authorization_name is called when exiting the session_authorization_name production.
	ExitSession_authorization_name(c *Session_authorization_nameContext)

	// ExitComponent_name is called when exiting the component_name production.
	ExitComponent_name(c *Component_nameContext)

	// ExitSec_label_comp_name is called when exiting the sec_label_comp_name production.
	ExitSec_label_comp_name(c *Sec_label_comp_nameContext)

	// ExitSecurity_policy_name is called when exiting the security_policy_name production.
	ExitSecurity_policy_name(c *Security_policy_nameContext)

	// ExitSecurity_label_name is called when exiting the security_label_name production.
	ExitSecurity_label_name(c *Security_label_nameContext)

	// ExitSequence_name is called when exiting the sequence_name production.
	ExitSequence_name(c *Sequence_nameContext)

	// ExitService_class_name is called when exiting the service_class_name production.
	ExitService_class_name(c *Service_class_nameContext)

	// ExitService_superclass_name is called when exiting the service_superclass_name production.
	ExitService_superclass_name(c *Service_superclass_nameContext)

	// ExitStoragegroup_name is called when exiting the storagegroup_name production.
	ExitStoragegroup_name(c *Storagegroup_nameContext)

	// ExitSupertype_name is called when exiting the supertype_name production.
	ExitSupertype_name(c *Supertype_nameContext)

	// ExitSuperview_name is called when exiting the superview_name production.
	ExitSuperview_name(c *Superview_nameContext)

	// ExitService_subclass_name is called when exiting the service_subclass_name production.
	ExitService_subclass_name(c *Service_subclass_nameContext)

	// ExitStatement_name is called when exiting the statement_name production.
	ExitStatement_name(c *Statement_nameContext)

	// ExitTable_name is called when exiting the table_name production.
	ExitTable_name(c *Table_nameContext)

	// ExitTablespace_name is called when exiting the tablespace_name production.
	ExitTablespace_name(c *Tablespace_nameContext)

	// ExitTarget_identifier is called when exiting the target_identifier production.
	ExitTarget_identifier(c *Target_identifierContext)

	// ExitThreshold_name is called when exiting the threshold_name production.
	ExitThreshold_name(c *Threshold_nameContext)

	// ExitTrigger_name is called when exiting the trigger_name production.
	ExitTrigger_name(c *Trigger_nameContext)

	// ExitContext_name is called when exiting the context_name production.
	ExitContext_name(c *Context_nameContext)

	// ExitUsage_list_name is called when exiting the usage_list_name production.
	ExitUsage_list_name(c *Usage_list_nameContext)

	// ExitType_name is called when exiting the type_name production.
	ExitType_name(c *Type_nameContext)

	// ExitType_mapping_name is called when exiting the type_mapping_name production.
	ExitType_mapping_name(c *Type_mapping_nameContext)

	// ExitTyped_table_name is called when exiting the typed_table_name production.
	ExitTyped_table_name(c *Typed_table_nameContext)

	// ExitTyped_view_name is called when exiting the typed_view_name production.
	ExitTyped_view_name(c *Typed_view_nameContext)

	// ExitUser_mapping_option_name is called when exiting the user_mapping_option_name production.
	ExitUser_mapping_option_name(c *User_mapping_option_nameContext)

	// ExitView_name is called when exiting the view_name production.
	ExitView_name(c *View_nameContext)

	// ExitVariable_name is called when exiting the variable_name production.
	ExitVariable_name(c *Variable_nameContext)

	// ExitWork_action_set_name is called when exiting the work_action_set_name production.
	ExitWork_action_set_name(c *Work_action_set_nameContext)

	// ExitWork_class_set_name is called when exiting the work_class_set_name production.
	ExitWork_class_set_name(c *Work_class_set_nameContext)

	// ExitWorkload_name is called when exiting the workload_name production.
	ExitWorkload_name(c *Workload_nameContext)

	// ExitWork_action_name is called when exiting the work_action_name production.
	ExitWork_action_name(c *Work_action_nameContext)

	// ExitWork_class_name is called when exiting the work_class_name production.
	ExitWork_class_name(c *Work_class_nameContext)

	// ExitWrapper_name is called when exiting the wrapper_name production.
	ExitWrapper_name(c *Wrapper_nameContext)

	// ExitWrapper_option_name is called when exiting the wrapper_option_name production.
	ExitWrapper_option_name(c *Wrapper_option_nameContext)

	// ExitXsrobject_name is called when exiting the xsrobject_name production.
	ExitXsrobject_name(c *Xsrobject_nameContext)

	// ExitParameter_name is called when exiting the parameter_name production.
	ExitParameter_name(c *Parameter_nameContext)

	// ExitCursor_variable_name is called when exiting the cursor_variable_name production.
	ExitCursor_variable_name(c *Cursor_variable_nameContext)

	// ExitAlias_name is called when exiting the alias_name production.
	ExitAlias_name(c *Alias_nameContext)

	// ExitDb_partition_group_name is called when exiting the db_partition_group_name production.
	ExitDb_partition_group_name(c *Db_partition_group_nameContext)

	// ExitSource_index_name is called when exiting the source_index_name production.
	ExitSource_index_name(c *Source_index_nameContext)

	// ExitSource_table_name is called when exiting the source_table_name production.
	ExitSource_table_name(c *Source_table_nameContext)

	// ExitSource_storagegroup_name is called when exiting the source_storagegroup_name production.
	ExitSource_storagegroup_name(c *Source_storagegroup_nameContext)

	// ExitTarget_storagegroup_name is called when exiting the target_storagegroup_name production.
	ExitTarget_storagegroup_name(c *Target_storagegroup_nameContext)

	// ExitSource_tablespace_name is called when exiting the source_tablespace_name production.
	ExitSource_tablespace_name(c *Source_tablespace_nameContext)

	// ExitTarget_tablespace_name is called when exiting the target_tablespace_name production.
	ExitTarget_tablespace_name(c *Target_tablespace_nameContext)

	// ExitUnqualified_function_name is called when exiting the unqualified_function_name production.
	ExitUnqualified_function_name(c *Unqualified_function_nameContext)

	// ExitUnqualified_procedure_name is called when exiting the unqualified_procedure_name production.
	ExitUnqualified_procedure_name(c *Unqualified_procedure_nameContext)

	// ExitUnqualified_specific_name is called when exiting the unqualified_specific_name production.
	ExitUnqualified_specific_name(c *Unqualified_specific_nameContext)

	// ExitPeriod_name is called when exiting the period_name production.
	ExitPeriod_name(c *Period_nameContext)

	// ExitHistory_table_name is called when exiting the history_table_name production.
	ExitHistory_table_name(c *History_table_nameContext)

	// ExitXml_schema_name is called when exiting the xml_schema_name production.
	ExitXml_schema_name(c *Xml_schema_nameContext)

	// ExitTodo is called when exiting the todo production.
	ExitTodo(c *TodoContext)
}
