// Code generated from Db2Parser.g4 by ANTLR 4.13.2. DO NOT EDIT.

package db2 // Db2Parser
import "github.com/antlr4-go/antlr/v4"

// A complete Visitor for a parse tree produced by Db2Parser.
type Db2ParserVisitor interface {
	antlr.ParseTreeVisitor

	// Visit a parse tree produced by Db2Parser#db2_file.
	VisitDb2_file(ctx *Db2_fileContext) interface{}

	// Visit a parse tree produced by Db2Parser#batch.
	VisitBatch(ctx *BatchContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_statement.
	VisitSql_statement(ctx *Sql_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_schema_statement.
	VisitSql_schema_statement(ctx *Sql_schema_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_data_change_statement.
	VisitSql_data_change_statement(ctx *Sql_data_change_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_data_statement.
	VisitSql_data_statement(ctx *Sql_data_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_transaction_statement.
	VisitSql_transaction_statement(ctx *Sql_transaction_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_connection_statement.
	VisitSql_connection_statement(ctx *Sql_connection_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_dynamic_statement.
	VisitSql_dynamic_statement(ctx *Sql_dynamic_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_session_statement.
	VisitSql_session_statement(ctx *Sql_session_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_embedded_host_language_statement.
	VisitSql_embedded_host_language_statement(ctx *Sql_embedded_host_language_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_constrol_statement.
	VisitSql_constrol_statement(ctx *Sql_constrol_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#select_statement.
	VisitSelect_statement(ctx *Select_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#read_only_clause.
	VisitRead_only_clause(ctx *Read_only_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#update_clause.
	VisitUpdate_clause(ctx *Update_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#optimize_for_clause.
	VisitOptimize_for_clause(ctx *Optimize_for_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#concurrent_access_resolution_clause.
	VisitConcurrent_access_resolution_clause(ctx *Concurrent_access_resolution_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#delete_statement.
	VisitDelete_statement(ctx *Delete_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#delete_statement_searched_delete.
	VisitDelete_statement_searched_delete(ctx *Delete_statement_searched_deleteContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_or_view_name.
	VisitTable_or_view_name(ctx *Table_or_view_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#delete_statement_positioned_delete.
	VisitDelete_statement_positioned_delete(ctx *Delete_statement_positioned_deleteContext) interface{}

	// Visit a parse tree produced by Db2Parser#delete_deltalake_statement.
	VisitDelete_deltalake_statement(ctx *Delete_deltalake_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#insert_statement.
	VisitInsert_statement(ctx *Insert_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#insert_datalake_statement.
	VisitInsert_datalake_statement(ctx *Insert_datalake_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#values_item.
	VisitValues_item(ctx *Values_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#merge_statement.
	VisitMerge_statement(ctx *Merge_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_view_fullselect.
	VisitTable_view_fullselect(ctx *Table_view_fullselectContext) interface{}

	// Visit a parse tree produced by Db2Parser#common_table_expression_list.
	VisitCommon_table_expression_list(ctx *Common_table_expression_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#matching_condition.
	VisitMatching_condition(ctx *Matching_conditionContext) interface{}

	// Visit a parse tree produced by Db2Parser#modification_operation.
	VisitModification_operation(ctx *Modification_operationContext) interface{}

	// Visit a parse tree produced by Db2Parser#update_operation.
	VisitUpdate_operation(ctx *Update_operationContext) interface{}

	// Visit a parse tree produced by Db2Parser#delete_operation.
	VisitDelete_operation(ctx *Delete_operationContext) interface{}

	// Visit a parse tree produced by Db2Parser#insert_operation.
	VisitInsert_operation(ctx *Insert_operationContext) interface{}

	// Visit a parse tree produced by Db2Parser#expr_null_default_list.
	VisitExpr_null_default_list(ctx *Expr_null_default_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#isolation_level.
	VisitIsolation_level(ctx *Isolation_levelContext) interface{}

	// Visit a parse tree produced by Db2Parser#truncate_statement.
	VisitTruncate_statement(ctx *Truncate_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#update_statement.
	VisitUpdate_statement(ctx *Update_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#update_statement_searched_update.
	VisitUpdate_statement_searched_update(ctx *Update_statement_searched_updateContext) interface{}

	// Visit a parse tree produced by Db2Parser#skip_wait.
	VisitSkip_wait(ctx *Skip_waitContext) interface{}

	// Visit a parse tree produced by Db2Parser#update_statement_positioned_update.
	VisitUpdate_statement_positioned_update(ctx *Update_statement_positioned_updateContext) interface{}

	// Visit a parse tree produced by Db2Parser#include_columns.
	VisitInclude_columns(ctx *Include_columnsContext) interface{}

	// Visit a parse tree produced by Db2Parser#assignment_clause.
	VisitAssignment_clause(ctx *Assignment_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#assignment_item.
	VisitAssignment_item(ctx *Assignment_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#period_clause.
	VisitPeriod_clause(ctx *Period_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#time_sec.
	VisitTime_sec(ctx *Time_secContext) interface{}

	// Visit a parse tree produced by Db2Parser#update_datalake_statement.
	VisitUpdate_datalake_statement(ctx *Update_datalake_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_database_authorities_statement.
	VisitGrant_database_authorities_statement(ctx *Grant_database_authorities_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_privilege_list.
	VisitDb_privilege_list(ctx *Db_privilege_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_privilege.
	VisitDb_privilege(ctx *Db_privilegeContext) interface{}

	// Visit a parse tree produced by Db2Parser#grantee.
	VisitGrantee(ctx *GranteeContext) interface{}

	// Visit a parse tree produced by Db2Parser#grantee_user_group.
	VisitGrantee_user_group(ctx *Grantee_user_groupContext) interface{}

	// Visit a parse tree produced by Db2Parser#user_group.
	VisitUser_group(ctx *User_groupContext) interface{}

	// Visit a parse tree produced by Db2Parser#grantee_list.
	VisitGrantee_list(ctx *Grantee_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#grantee_list_public.
	VisitGrantee_list_public(ctx *Grantee_list_publicContext) interface{}

	// Visit a parse tree produced by Db2Parser#grantee_list_user_group.
	VisitGrantee_list_user_group(ctx *Grantee_list_user_groupContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_exemption_statement.
	VisitGrant_exemption_statement(ctx *Grant_exemption_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#exemption_privilege.
	VisitExemption_privilege(ctx *Exemption_privilegeContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_global_variable_privileges_statement.
	VisitGrant_global_variable_privileges_statement(ctx *Grant_global_variable_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#variable_privilege.
	VisitVariable_privilege(ctx *Variable_privilegeContext) interface{}

	// Visit a parse tree produced by Db2Parser#read_write.
	VisitRead_write(ctx *Read_writeContext) interface{}

	// Visit a parse tree produced by Db2Parser#with_grant_option.
	VisitWith_grant_option(ctx *With_grant_optionContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_index_privileges_statement.
	VisitGrant_index_privileges_statement(ctx *Grant_index_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_module_privileges_statement.
	VisitGrant_module_privileges_statement(ctx *Grant_module_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_package_privileges_statement.
	VisitGrant_package_privileges_statement(ctx *Grant_package_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#package_privilege_list.
	VisitPackage_privilege_list(ctx *Package_privilege_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#package_privilege.
	VisitPackage_privilege(ctx *Package_privilegeContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_role_statement.
	VisitGrant_role_statement(ctx *Grant_role_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#role_list.
	VisitRole_list(ctx *Role_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_routine_privileges_statement.
	VisitGrant_routine_privileges_statement(ctx *Grant_routine_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_schema_privileges_statement.
	VisitGrant_schema_privileges_statement(ctx *Grant_schema_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#schema_privilege_list.
	VisitSchema_privilege_list(ctx *Schema_privilege_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#schema_privilege.
	VisitSchema_privilege(ctx *Schema_privilegeContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_security_label_statement.
	VisitGrant_security_label_statement(ctx *Grant_security_label_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_sequence_privileges_statement.
	VisitGrant_sequence_privileges_statement(ctx *Grant_sequence_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sequence_privilege_list.
	VisitSequence_privilege_list(ctx *Sequence_privilege_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#sequence_privilege.
	VisitSequence_privilege(ctx *Sequence_privilegeContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_server_privileges_statement.
	VisitGrant_server_privileges_statement(ctx *Grant_server_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_setsessionuser_privilege_statement.
	VisitGrant_setsessionuser_privilege_statement(ctx *Grant_setsessionuser_privilege_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#user_list.
	VisitUser_list(ctx *User_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#user_auth.
	VisitUser_auth(ctx *User_authContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_table_space_privileges_statement.
	VisitGrant_table_space_privileges_statement(ctx *Grant_table_space_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_table_view_or_nickname_privileges_statement.
	VisitGrant_table_view_or_nickname_privileges_statement(ctx *Grant_table_view_or_nickname_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#tvn_privilege_list.
	VisitTvn_privilege_list(ctx *Tvn_privilege_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#tvn_privilege.
	VisitTvn_privilege(ctx *Tvn_privilegeContext) interface{}

	// Visit a parse tree produced by Db2Parser#column_name_list_paren.
	VisitColumn_name_list_paren(ctx *Column_name_list_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#column_name_list.
	VisitColumn_name_list(ctx *Column_name_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_workload_privileges_statement.
	VisitGrant_workload_privileges_statement(ctx *Grant_workload_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_xsr_object_privileges_statement.
	VisitGrant_xsr_object_privileges_statement(ctx *Grant_xsr_object_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_database_authorities_statement.
	VisitRevoke_database_authorities_statement(ctx *Revoke_database_authorities_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#by_all.
	VisitBy_all(ctx *By_allContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_exemption_statement.
	VisitRevoke_exemption_statement(ctx *Revoke_exemption_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_global_variable_privileges_statement.
	VisitRevoke_global_variable_privileges_statement(ctx *Revoke_global_variable_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_index_privileges_statement.
	VisitRevoke_index_privileges_statement(ctx *Revoke_index_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_module_privileges_statement.
	VisitRevoke_module_privileges_statement(ctx *Revoke_module_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_package_privileges_statement.
	VisitRevoke_package_privileges_statement(ctx *Revoke_package_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_role_statement.
	VisitRevoke_role_statement(ctx *Revoke_role_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_routine_privileges_statement.
	VisitRevoke_routine_privileges_statement(ctx *Revoke_routine_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_schema_privileges_statement.
	VisitRevoke_schema_privileges_statement(ctx *Revoke_schema_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_security_label_statement.
	VisitRevoke_security_label_statement(ctx *Revoke_security_label_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_sequence_privileges_statement.
	VisitRevoke_sequence_privileges_statement(ctx *Revoke_sequence_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_server_privileges_statement.
	VisitRevoke_server_privileges_statement(ctx *Revoke_server_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_setsessionuser_privilege_statement.
	VisitRevoke_setsessionuser_privilege_statement(ctx *Revoke_setsessionuser_privilege_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_table_space_privileges_statement.
	VisitRevoke_table_space_privileges_statement(ctx *Revoke_table_space_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_table_view_or_nickname_privileges_statement.
	VisitRevoke_table_view_or_nickname_privileges_statement(ctx *Revoke_table_view_or_nickname_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_workload_privileges_statement.
	VisitRevoke_workload_privileges_statement(ctx *Revoke_workload_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#revoke_xsr_object_privileges_statement.
	VisitRevoke_xsr_object_privileges_statement(ctx *Revoke_xsr_object_privileges_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#user_group_role.
	VisitUser_group_role(ctx *User_group_roleContext) interface{}

	// Visit a parse tree produced by Db2Parser#rollback_statement.
	VisitRollback_statement(ctx *Rollback_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#savepoint_statement.
	VisitSavepoint_statement(ctx *Savepoint_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#release_savepoint_statement.
	VisitRelease_savepoint_statement(ctx *Release_savepoint_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#allocate_cursor_statement.
	VisitAllocate_cursor_statement(ctx *Allocate_cursor_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_audit_policy_statement.
	VisitAlter_audit_policy_statement(ctx *Alter_audit_policy_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#status_spec.
	VisitStatus_spec(ctx *Status_specContext) interface{}

	// Visit a parse tree produced by Db2Parser#normal_audit.
	VisitNormal_audit(ctx *Normal_auditContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_bufferpool_statement.
	VisitAlter_bufferpool_statement(ctx *Alter_bufferpool_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#immediate_deferred.
	VisitImmediate_deferred(ctx *Immediate_deferredContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_database_partition_group_statement.
	VisitAlter_database_partition_group_statement(ctx *Alter_database_partition_group_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_partition_group_list_item.
	VisitDb_partition_group_list_item(ctx *Db_partition_group_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_partition_num_nums.
	VisitDb_partition_num_nums(ctx *Db_partition_num_numsContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_partitions_clause.
	VisitDb_partitions_clause(ctx *Db_partitions_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_partition_options.
	VisitDb_partition_options(ctx *Db_partition_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_database_statement.
	VisitAlter_database_statement(ctx *Alter_database_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_database_opts.
	VisitAlter_database_opts(ctx *Alter_database_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_event_monitor_statement.
	VisitAlter_event_monitor_statement(ctx *Alter_event_monitor_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_event_monitor_opts.
	VisitAlter_event_monitor_opts(ctx *Alter_event_monitor_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_function_statement.
	VisitAlter_function_statement(ctx *Alter_function_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_function_opts.
	VisitAlter_function_opts(ctx *Alter_function_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#function_designator.
	VisitFunction_designator(ctx *Function_designatorContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_type_list.
	VisitData_type_list(ctx *Data_type_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_type_list_paren.
	VisitData_type_list_paren(ctx *Data_type_list_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_histogram_template_statement.
	VisitAlter_histogram_template_statement(ctx *Alter_histogram_template_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_index_statement.
	VisitAlter_index_statement(ctx *Alter_index_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#yes_no.
	VisitYes_no(ctx *Yes_noContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_mask_statement.
	VisitAlter_mask_statement(ctx *Alter_mask_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#enable_disable.
	VisitEnable_disable(ctx *Enable_disableContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_method_statement.
	VisitAlter_method_statement(ctx *Alter_method_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_designator.
	VisitMethod_designator(ctx *Method_designatorContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_model_statement.
	VisitAlter_model_statement(ctx *Alter_model_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_module_statement.
	VisitAlter_module_statement(ctx *Alter_module_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_module_opts.
	VisitAlter_module_opts(ctx *Alter_module_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#module_function_definition.
	VisitModule_function_definition(ctx *Module_function_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#module_procedure_definition.
	VisitModule_procedure_definition(ctx *Module_procedure_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#module_type_definition.
	VisitModule_type_definition(ctx *Module_type_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#module_variable_definition.
	VisitModule_variable_definition(ctx *Module_variable_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#module_condition_definition.
	VisitModule_condition_definition(ctx *Module_condition_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#module_object_identification.
	VisitModule_object_identification(ctx *Module_object_identificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#module_function_designator.
	VisitModule_function_designator(ctx *Module_function_designatorContext) interface{}

	// Visit a parse tree produced by Db2Parser#module_procedure_designator.
	VisitModule_procedure_designator(ctx *Module_procedure_designatorContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_nickname_statement.
	VisitAlter_nickname_statement(ctx *Alter_nickname_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_nickname_opts_1.
	VisitAlter_nickname_opts_1(ctx *Alter_nickname_opts_1Context) interface{}

	// Visit a parse tree produced by Db2Parser#alter_nickname_opts_1_item.
	VisitAlter_nickname_opts_1_item(ctx *Alter_nickname_opts_1_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_nickname_opts_2.
	VisitAlter_nickname_opts_2(ctx *Alter_nickname_opts_2Context) interface{}

	// Visit a parse tree produced by Db2Parser#alter_nickname_opts_2_item.
	VisitAlter_nickname_opts_2_item(ctx *Alter_nickname_opts_2_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#constraint_alteration.
	VisitConstraint_alteration(ctx *Constraint_alterationContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_package_statement.
	VisitAlter_package_statement(ctx *Alter_package_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_package_opts.
	VisitAlter_package_opts(ctx *Alter_package_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_permission_statement.
	VisitAlter_permission_statement(ctx *Alter_permission_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_procedure_external_statement.
	VisitAlter_procedure_external_statement(ctx *Alter_procedure_external_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_procedure_external_opts.
	VisitAlter_procedure_external_opts(ctx *Alter_procedure_external_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#procedure_designator.
	VisitProcedure_designator(ctx *Procedure_designatorContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_procedure_sourced_statement.
	VisitAlter_procedure_sourced_statement(ctx *Alter_procedure_sourced_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#parameter_alteration.
	VisitParameter_alteration(ctx *Parameter_alterationContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_procedure_sql_statement.
	VisitAlter_procedure_sql_statement(ctx *Alter_procedure_sql_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_schema_statement.
	VisitAlter_schema_statement(ctx *Alter_schema_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#none_changes.
	VisitNone_changes(ctx *None_changesContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_security_label_component_statement.
	VisitAlter_security_label_component_statement(ctx *Alter_security_label_component_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#add_element_clause.
	VisitAdd_element_clause(ctx *Add_element_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#array_element_clause.
	VisitArray_element_clause(ctx *Array_element_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#tree_element_clause.
	VisitTree_element_clause(ctx *Tree_element_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_security_policy_statement.
	VisitAlter_security_policy_statement(ctx *Alter_security_policy_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_security_policy_opts.
	VisitAlter_security_policy_opts(ctx *Alter_security_policy_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_sequence_statement.
	VisitAlter_sequence_statement(ctx *Alter_sequence_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_sequence_opts.
	VisitAlter_sequence_opts(ctx *Alter_sequence_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_server_statement.
	VisitAlter_server_statement(ctx *Alter_server_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_server_opts.
	VisitAlter_server_opts(ctx *Alter_server_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_service_class_statement.
	VisitAlter_service_class_statement(ctx *Alter_service_class_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_service_class_opts.
	VisitAlter_service_class_opts(ctx *Alter_service_class_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#default_on_off.
	VisitDefault_on_off(ctx *Default_on_offContext) interface{}

	// Visit a parse tree produced by Db2Parser#default_high_medium_low.
	VisitDefault_high_medium_low(ctx *Default_high_medium_lowContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_stogroup_statement.
	VisitAlter_stogroup_statement(ctx *Alter_stogroup_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_stogroup_opts.
	VisitAlter_stogroup_opts(ctx *Alter_stogroup_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_table_statement.
	VisitAlter_table_statement(ctx *Alter_table_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_table_opts.
	VisitAlter_table_opts(ctx *Alter_table_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#null_on_off.
	VisitNull_on_off(ctx *Null_on_offContext) interface{}

	// Visit a parse tree produced by Db2Parser#cascade_restrict.
	VisitCascade_restrict(ctx *Cascade_restrictContext) interface{}

	// Visit a parse tree produced by Db2Parser#materialized_query_definition.
	VisitMaterialized_query_definition(ctx *Materialized_query_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#refreshable_table_options.
	VisitRefreshable_table_options(ctx *Refreshable_table_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#column_alteration.
	VisitColumn_alteration(ctx *Column_alterationContext) interface{}

	// Visit a parse tree produced by Db2Parser#generation_alteration.
	VisitGeneration_alteration(ctx *Generation_alterationContext) interface{}

	// Visit a parse tree produced by Db2Parser#identity_alteration.
	VisitIdentity_alteration(ctx *Identity_alterationContext) interface{}

	// Visit a parse tree produced by Db2Parser#generation_attribute.
	VisitGeneration_attribute(ctx *Generation_attributeContext) interface{}

	// Visit a parse tree produced by Db2Parser#as_identity_clause.
	VisitAs_identity_clause(ctx *As_identity_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#as_identity_clause_opts.
	VisitAs_identity_clause_opts(ctx *As_identity_clause_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#period_definition_alter.
	VisitPeriod_definition_alter(ctx *Period_definition_alterContext) interface{}

	// Visit a parse tree produced by Db2Parser#add_partition.
	VisitAdd_partition(ctx *Add_partitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#boundary_spec_alter.
	VisitBoundary_spec_alter(ctx *Boundary_spec_alterContext) interface{}

	// Visit a parse tree produced by Db2Parser#attach_partition.
	VisitAttach_partition(ctx *Attach_partitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#activate_deactivate.
	VisitActivate_deactivate(ctx *Activate_deactivateContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_tablespace_statement.
	VisitAlter_tablespace_statement(ctx *Alter_tablespace_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_tablespace_opts.
	VisitAlter_tablespace_opts(ctx *Alter_tablespace_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#add_clause.
	VisitAdd_clause(ctx *Add_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_container_clause.
	VisitDb_container_clause(ctx *Db_container_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_container_clause_opts.
	VisitDb_container_clause_opts(ctx *Db_container_clause_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#drop_container_clause.
	VisitDrop_container_clause(ctx *Drop_container_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#file_device.
	VisitFile_device(ctx *File_deviceContext) interface{}

	// Visit a parse tree produced by Db2Parser#all_containers_clause.
	VisitAll_containers_clause(ctx *All_containers_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#system_container_clause.
	VisitSystem_container_clause(ctx *System_container_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#stripeset.
	VisitStripeset(ctx *StripesetContext) interface{}

	// Visit a parse tree produced by Db2Parser#km.
	VisitKm(ctx *KmContext) interface{}

	// Visit a parse tree produced by Db2Parser#kmg_percent.
	VisitKmg_percent(ctx *Kmg_percentContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_threshold_statement.
	VisitAlter_threshold_statement(ctx *Alter_threshold_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_threshold_opts.
	VisitAlter_threshold_opts(ctx *Alter_threshold_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_threshold_predicate.
	VisitAlter_threshold_predicate(ctx *Alter_threshold_predicateContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_threshold_exceeded_actions.
	VisitAlter_threshold_exceeded_actions(ctx *Alter_threshold_exceeded_actionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#dt_units.
	VisitDt_units(ctx *Dt_unitsContext) interface{}

	// Visit a parse tree produced by Db2Parser#dt_units_with_seconds.
	VisitDt_units_with_seconds(ctx *Dt_units_with_secondsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_trigger_statement.
	VisitAlter_trigger_statement(ctx *Alter_trigger_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_trusted_context_statement.
	VisitAlter_trusted_context_statement(ctx *Alter_trusted_context_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_trusted_context_opts.
	VisitAlter_trusted_context_opts(ctx *Alter_trusted_context_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_trusted_context_opts_alter_opts.
	VisitAlter_trusted_context_opts_alter_opts(ctx *Alter_trusted_context_opts_alter_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#addr_clause_encryption_val.
	VisitAddr_clause_encryption_val(ctx *Addr_clause_encryption_valContext) interface{}

	// Visit a parse tree produced by Db2Parser#address_clause.
	VisitAddress_clause(ctx *Address_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#user_clause.
	VisitUser_clause(ctx *User_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#use_for_opts.
	VisitUse_for_opts(ctx *Use_for_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#use_for_opts_2.
	VisitUse_for_opts_2(ctx *Use_for_opts_2Context) interface{}

	// Visit a parse tree produced by Db2Parser#alter_type_statement.
	VisitAlter_type_statement(ctx *Alter_type_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_type_opts.
	VisitAlter_type_opts(ctx *Alter_type_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_identifier.
	VisitMethod_identifier(ctx *Method_identifierContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_options.
	VisitMethod_options(ctx *Method_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_usage_list_statement.
	VisitAlter_usage_list_statement(ctx *Alter_usage_list_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_usage_list_opts_item.
	VisitAlter_usage_list_opts_item(ctx *Alter_usage_list_opts_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_user_mapping_statement.
	VisitAlter_user_mapping_statement(ctx *Alter_user_mapping_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_user_mapping_opts_item.
	VisitAlter_user_mapping_opts_item(ctx *Alter_user_mapping_opts_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#add_set.
	VisitAdd_set(ctx *Add_setContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_view_statement.
	VisitAlter_view_statement(ctx *Alter_view_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_view_opts.
	VisitAlter_view_opts(ctx *Alter_view_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_work_action_set_statement.
	VisitAlter_work_action_set_statement(ctx *Alter_work_action_set_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_work_action_set_opts.
	VisitAlter_work_action_set_opts(ctx *Alter_work_action_set_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_action_alteration.
	VisitWork_action_alteration(ctx *Work_action_alterationContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_action_alteration_opts.
	VisitWork_action_alteration_opts(ctx *Work_action_alteration_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_action_types_clause.
	VisitAlter_action_types_clause(ctx *Alter_action_types_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#threshold_predicate_clause.
	VisitThreshold_predicate_clause(ctx *Threshold_predicate_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_work_class_set_statement.
	VisitAlter_work_class_set_statement(ctx *Alter_work_class_set_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_work_class_set_opts.
	VisitAlter_work_class_set_opts(ctx *Alter_work_class_set_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_class_alteration.
	VisitWork_class_alteration(ctx *Work_class_alterationContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_class_alteration_opts.
	VisitWork_class_alteration_opts(ctx *Work_class_alteration_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#for_from_to_alter_clause.
	VisitFor_from_to_alter_clause(ctx *For_from_to_alter_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#schema_alter_clause.
	VisitSchema_alter_clause(ctx *Schema_alter_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_tag_alter_clause.
	VisitData_tag_alter_clause(ctx *Data_tag_alter_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_workload_statement.
	VisitAlter_workload_statement(ctx *Alter_workload_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_workload_opts_item.
	VisitAlter_workload_opts_item(ctx *Alter_workload_opts_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#package_executable.
	VisitPackage_executable(ctx *Package_executableContext) interface{}

	// Visit a parse tree produced by Db2Parser#base_none.
	VisitBase_none(ctx *Base_noneContext) interface{}

	// Visit a parse tree produced by Db2Parser#extended_base_none.
	VisitExtended_base_none(ctx *Extended_base_noneContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_collect_activity_data_clause.
	VisitAlter_collect_activity_data_clause(ctx *Alter_collect_activity_data_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#with_opts.
	VisitWith_opts(ctx *With_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_collect_history_clause.
	VisitAlter_collect_history_clause(ctx *Alter_collect_history_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_collect_lock_wait_data_clause.
	VisitAlter_collect_lock_wait_data_clause(ctx *Alter_collect_lock_wait_data_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_wrapper_statement.
	VisitAlter_wrapper_statement(ctx *Alter_wrapper_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_wrapper_opts_item.
	VisitAlter_wrapper_opts_item(ctx *Alter_wrapper_opts_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#alter_xsrobject_statement.
	VisitAlter_xsrobject_statement(ctx *Alter_xsrobject_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#string.
	VisitString(ctx *StringContext) interface{}

	// Visit a parse tree produced by Db2Parser#string_constant.
	VisitString_constant(ctx *String_constantContext) interface{}

	// Visit a parse tree produced by Db2Parser#numeric_constant.
	VisitNumeric_constant(ctx *Numeric_constantContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_type.
	VisitData_type(ctx *Data_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#anchored_data_type.
	VisitAnchored_data_type(ctx *Anchored_data_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#anchored_non_row_data_type.
	VisitAnchored_non_row_data_type(ctx *Anchored_non_row_data_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#anchored_row_data_type.
	VisitAnchored_row_data_type(ctx *Anchored_row_data_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#source_data_type.
	VisitSource_data_type(ctx *Source_data_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_type_constrainst.
	VisitData_type_constrainst(ctx *Data_type_constrainstContext) interface{}

	// Visit a parse tree produced by Db2Parser#check_condition.
	VisitCheck_condition(ctx *Check_conditionContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_type_2.
	VisitData_type_2(ctx *Data_type_2Context) interface{}

	// Visit a parse tree produced by Db2Parser#built_in_type.
	VisitBuilt_in_type(ctx *Built_in_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#integer_paren.
	VisitInteger_paren(ctx *Integer_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#integer_kmg_paren.
	VisitInteger_kmg_paren(ctx *Integer_kmg_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#char_character.
	VisitChar_character(ctx *Char_characterContext) interface{}

	// Visit a parse tree produced by Db2Parser#octets_codeunits.
	VisitOctets_codeunits(ctx *Octets_codeunitsContext) interface{}

	// Visit a parse tree produced by Db2Parser#codeunits.
	VisitCodeunits(ctx *CodeunitsContext) interface{}

	// Visit a parse tree produced by Db2Parser#kmg.
	VisitKmg(ctx *KmgContext) interface{}

	// Visit a parse tree produced by Db2Parser#rs_locator_variable.
	VisitRs_locator_variable(ctx *Rs_locator_variableContext) interface{}

	// Visit a parse tree produced by Db2Parser#integer_constant_list.
	VisitInteger_constant_list(ctx *Integer_constant_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#integer_constant.
	VisitInteger_constant(ctx *Integer_constantContext) interface{}

	// Visit a parse tree produced by Db2Parser#integer_value.
	VisitInteger_value(ctx *Integer_valueContext) interface{}

	// Visit a parse tree produced by Db2Parser#positive_integer.
	VisitPositive_integer(ctx *Positive_integerContext) interface{}

	// Visit a parse tree produced by Db2Parser#bigint_value.
	VisitBigint_value(ctx *Bigint_valueContext) interface{}

	// Visit a parse tree produced by Db2Parser#bigint_constant.
	VisitBigint_constant(ctx *Bigint_constantContext) interface{}

	// Visit a parse tree produced by Db2Parser#member_number.
	VisitMember_number(ctx *Member_numberContext) interface{}

	// Visit a parse tree produced by Db2Parser#version_id.
	VisitVersion_id(ctx *Version_idContext) interface{}

	// Visit a parse tree produced by Db2Parser#drop_statement.
	VisitDrop_statement(ctx *Drop_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#alias_designator.
	VisitAlias_designator(ctx *Alias_designatorContext) interface{}

	// Visit a parse tree produced by Db2Parser#service_class_designator.
	VisitService_class_designator(ctx *Service_class_designatorContext) interface{}

	// Visit a parse tree produced by Db2Parser#tablespace_name_list.
	VisitTablespace_name_list(ctx *Tablespace_name_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#associate_locators_statement.
	VisitAssociate_locators_statement(ctx *Associate_locators_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#audit_statement.
	VisitAudit_statement(ctx *Audit_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#begin_declare_section_statement.
	VisitBegin_declare_section_statement(ctx *Begin_declare_section_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#call_statement.
	VisitCall_statement(ctx *Call_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#arg_list_paren.
	VisitArg_list_paren(ctx *Arg_list_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#arg_list.
	VisitArg_list(ctx *Arg_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#argument.
	VisitArgument(ctx *ArgumentContext) interface{}

	// Visit a parse tree produced by Db2Parser#case_statement.
	VisitCase_statement(ctx *Case_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#searched_case_statement_when_clause.
	VisitSearched_case_statement_when_clause(ctx *Searched_case_statement_when_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#simple_case_statement_when_clause.
	VisitSimple_case_statement_when_clause(ctx *Simple_case_statement_when_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#close_statement.
	VisitClose_statement(ctx *Close_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#comment_statement.
	VisitComment_statement(ctx *Comment_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#column_comment.
	VisitColumn_comment(ctx *Column_commentContext) interface{}

	// Visit a parse tree produced by Db2Parser#comment_objects.
	VisitComment_objects(ctx *Comment_objectsContext) interface{}

	// Visit a parse tree produced by Db2Parser#commit_statement.
	VisitCommit_statement(ctx *Commit_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#connect_type_1_statement.
	VisitConnect_type_1_statement(ctx *Connect_type_1_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#authorization.
	VisitAuthorization(ctx *AuthorizationContext) interface{}

	// Visit a parse tree produced by Db2Parser#passwords.
	VisitPasswords(ctx *PasswordsContext) interface{}

	// Visit a parse tree produced by Db2Parser#lock_block.
	VisitLock_block(ctx *Lock_blockContext) interface{}

	// Visit a parse tree produced by Db2Parser#accesstoken.
	VisitAccesstoken(ctx *AccesstokenContext) interface{}

	// Visit a parse tree produced by Db2Parser#token.
	VisitToken(ctx *TokenContext) interface{}

	// Visit a parse tree produced by Db2Parser#api_key.
	VisitApi_key(ctx *Api_keyContext) interface{}

	// Visit a parse tree produced by Db2Parser#token_type.
	VisitToken_type(ctx *Token_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#declare_cursor_statement.
	VisitDeclare_cursor_statement(ctx *Declare_cursor_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#declare_global_temporary_table_statement.
	VisitDeclare_global_temporary_table_statement(ctx *Declare_global_temporary_table_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#describe_statement.
	VisitDescribe_statement(ctx *Describe_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#xquery_statement.
	VisitXquery_statement(ctx *Xquery_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#describe_input_statement.
	VisitDescribe_input_statement(ctx *Describe_input_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#describe_output_statement.
	VisitDescribe_output_statement(ctx *Describe_output_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#disconnect_statement.
	VisitDisconnect_statement(ctx *Disconnect_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#end_declare_section_statement.
	VisitEnd_declare_section_statement(ctx *End_declare_section_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#execute_statement.
	VisitExecute_statement(ctx *Execute_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#host_variable_expression.
	VisitHost_variable_expression(ctx *Host_variable_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#assignment_target.
	VisitAssignment_target(ctx *Assignment_targetContext) interface{}

	// Visit a parse tree produced by Db2Parser#execute_immediate_statement.
	VisitExecute_immediate_statement(ctx *Execute_immediate_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#explain_statement.
	VisitExplain_statement(ctx *Explain_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#explainable_sql_statement.
	VisitExplainable_sql_statement(ctx *Explainable_sql_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#fetch_statement.
	VisitFetch_statement(ctx *Fetch_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#flush_bufferpools_statement.
	VisitFlush_bufferpools_statement(ctx *Flush_bufferpools_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#flush_event_monitor_statement.
	VisitFlush_event_monitor_statement(ctx *Flush_event_monitor_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#flush_federated_cache_statement.
	VisitFlush_federated_cache_statement(ctx *Flush_federated_cache_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#flush_optimization_profile_cache_statement.
	VisitFlush_optimization_profile_cache_statement(ctx *Flush_optimization_profile_cache_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#flush_package_cache_statement.
	VisitFlush_package_cache_statement(ctx *Flush_package_cache_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#flush_authentication_cache_statement.
	VisitFlush_authentication_cache_statement(ctx *Flush_authentication_cache_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#free_locator_statement.
	VisitFree_locator_statement(ctx *Free_locator_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#get_diagnostics_statement.
	VisitGet_diagnostics_statement(ctx *Get_diagnostics_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#statement_information.
	VisitStatement_information(ctx *Statement_informationContext) interface{}

	// Visit a parse tree produced by Db2Parser#condition_information.
	VisitCondition_information(ctx *Condition_informationContext) interface{}

	// Visit a parse tree produced by Db2Parser#condition_var_assignment.
	VisitCondition_var_assignment(ctx *Condition_var_assignmentContext) interface{}

	// Visit a parse tree produced by Db2Parser#lock_table_statement.
	VisitLock_table_statement(ctx *Lock_table_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#pipe_statement.
	VisitPipe_statement(ctx *Pipe_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#refresh_table_statement.
	VisitRefresh_table_statement(ctx *Refresh_table_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#release_connection_statement.
	VisitRelease_connection_statement(ctx *Release_connection_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#rename_statement.
	VisitRename_statement(ctx *Rename_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#rename_stogroup_statement.
	VisitRename_stogroup_statement(ctx *Rename_stogroup_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#rename_tablespace_statement.
	VisitRename_tablespace_statement(ctx *Rename_tablespace_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#set_statement.
	VisitSet_statement(ctx *Set_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#access_mode_clause.
	VisitAccess_mode_clause(ctx *Access_mode_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#cascade_clause.
	VisitCascade_clause(ctx *Cascade_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#to_descendent_types.
	VisitTo_descendent_types(ctx *To_descendent_typesContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_type_list.
	VisitTable_type_list(ctx *Table_type_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_type.
	VisitTable_type(ctx *Table_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_checked_options_list.
	VisitTable_checked_options_list(ctx *Table_checked_options_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_checked_options.
	VisitTable_checked_options(ctx *Table_checked_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#online_options.
	VisitOnline_options(ctx *Online_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#query_optimization_options.
	VisitQuery_optimization_options(ctx *Query_optimization_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#check_options.
	VisitCheck_options(ctx *Check_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#incremental_options.
	VisitIncremental_options(ctx *Incremental_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#exception_clause.
	VisitException_clause(ctx *Exception_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#in_table_use_clause.
	VisitIn_table_use_clause(ctx *In_table_use_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_unchecked_options.
	VisitTable_unchecked_options(ctx *Table_unchecked_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#full_access.
	VisitFull_access(ctx *Full_accessContext) interface{}

	// Visit a parse tree produced by Db2Parser#integrity_options.
	VisitIntegrity_options(ctx *Integrity_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#integrity_options_item.
	VisitIntegrity_options_item(ctx *Integrity_options_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#var_def_list.
	VisitVar_def_list(ctx *Var_def_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#var_def.
	VisitVar_def(ctx *Var_defContext) interface{}

	// Visit a parse tree produced by Db2Parser#expr_null.
	VisitExpr_null(ctx *Expr_nullContext) interface{}

	// Visit a parse tree produced by Db2Parser#expr_null_default.
	VisitExpr_null_default(ctx *Expr_null_defaultContext) interface{}

	// Visit a parse tree produced by Db2Parser#array_index.
	VisitArray_index(ctx *Array_indexContext) interface{}

	// Visit a parse tree produced by Db2Parser#row_fullselect.
	VisitRow_fullselect(ctx *Row_fullselectContext) interface{}

	// Visit a parse tree produced by Db2Parser#target_variable.
	VisitTarget_variable(ctx *Target_variableContext) interface{}

	// Visit a parse tree produced by Db2Parser#target_cursor_variable.
	VisitTarget_cursor_variable(ctx *Target_cursor_variableContext) interface{}

	// Visit a parse tree produced by Db2Parser#target_row_variable.
	VisitTarget_row_variable(ctx *Target_row_variableContext) interface{}

	// Visit a parse tree produced by Db2Parser#row_array_element_specification.
	VisitRow_array_element_specification(ctx *Row_array_element_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#row_field_reference.
	VisitRow_field_reference(ctx *Row_field_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#field_reference.
	VisitField_reference(ctx *Field_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#search_condition.
	VisitSearch_condition(ctx *Search_conditionContext) interface{}

	// Visit a parse tree produced by Db2Parser#predicate.
	VisitPredicate(ctx *PredicateContext) interface{}

	// Visit a parse tree produced by Db2Parser#according_to_clause.
	VisitAccording_to_clause(ctx *According_to_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#xml_schema_identification_list.
	VisitXml_schema_identification_list(ctx *Xml_schema_identification_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#xml_schema_identification.
	VisitXml_schema_identification(ctx *Xml_schema_identificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#fullselect_in_parentheses.
	VisitFullselect_in_parentheses(ctx *Fullselect_in_parenthesesContext) interface{}

	// Visit a parse tree produced by Db2Parser#some_any_all.
	VisitSome_any_all(ctx *Some_any_allContext) interface{}

	// Visit a parse tree produced by Db2Parser#row_value_expression.
	VisitRow_value_expression(ctx *Row_value_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#comparison_operator.
	VisitComparison_operator(ctx *Comparison_operatorContext) interface{}

	// Visit a parse tree produced by Db2Parser#row_expression.
	VisitRow_expression(ctx *Row_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#path_opt_list.
	VisitPath_opt_list(ctx *Path_opt_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#path_opt.
	VisitPath_opt(ctx *Path_optContext) interface{}

	// Visit a parse tree produced by Db2Parser#pkg_opt_list.
	VisitPkg_opt_list(ctx *Pkg_opt_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#pkg_opt.
	VisitPkg_opt(ctx *Pkg_optContext) interface{}

	// Visit a parse tree produced by Db2Parser#maintain_opt_list.
	VisitMaintain_opt_list(ctx *Maintain_opt_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#maintain_opt.
	VisitMaintain_opt(ctx *Maintain_optContext) interface{}

	// Visit a parse tree produced by Db2Parser#variable.
	VisitVariable(ctx *VariableContext) interface{}

	// Visit a parse tree produced by Db2Parser#host_variable.
	VisitHost_variable(ctx *Host_variableContext) interface{}

	// Visit a parse tree produced by Db2Parser#set_integrity_statement.
	VisitSet_integrity_statement(ctx *Set_integrity_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#transfer_ownership_statement.
	VisitTransfer_ownership_statement(ctx *Transfer_ownership_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#objects.
	VisitObjects(ctx *ObjectsContext) interface{}

	// Visit a parse tree produced by Db2Parser#whenever_statement.
	VisitWhenever_statement(ctx *Whenever_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#for_statement.
	VisitFor_statement(ctx *For_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#goto_statement.
	VisitGoto_statement(ctx *Goto_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#if_statement.
	VisitIf_statement(ctx *If_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#include_statement.
	VisitInclude_statement(ctx *Include_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#resignal_statement.
	VisitResignal_statement(ctx *Resignal_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#signal_information.
	VisitSignal_information(ctx *Signal_informationContext) interface{}

	// Visit a parse tree produced by Db2Parser#diagnostic_string_constant.
	VisitDiagnostic_string_constant(ctx *Diagnostic_string_constantContext) interface{}

	// Visit a parse tree produced by Db2Parser#signal_statement.
	VisitSignal_statement(ctx *Signal_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sqlstate_string_constant.
	VisitSqlstate_string_constant(ctx *Sqlstate_string_constantContext) interface{}

	// Visit a parse tree produced by Db2Parser#sqlstate_string_variable.
	VisitSqlstate_string_variable(ctx *Sqlstate_string_variableContext) interface{}

	// Visit a parse tree produced by Db2Parser#signal_information_2.
	VisitSignal_information_2(ctx *Signal_information_2Context) interface{}

	// Visit a parse tree produced by Db2Parser#diagnostic_string_expression.
	VisitDiagnostic_string_expression(ctx *Diagnostic_string_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#iterate_statement.
	VisitIterate_statement(ctx *Iterate_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#leave_statement.
	VisitLeave_statement(ctx *Leave_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#loop_statement.
	VisitLoop_statement(ctx *Loop_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#open_statement.
	VisitOpen_statement(ctx *Open_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#variable_or_expression.
	VisitVariable_or_expression(ctx *Variable_or_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#select_into_statement.
	VisitSelect_into_statement(ctx *Select_into_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#values_into_statement.
	VisitValues_into_statement(ctx *Values_into_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#prepare_statement.
	VisitPrepare_statement(ctx *Prepare_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#repeat_statement.
	VisitRepeat_statement(ctx *Repeat_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#return_statement.
	VisitReturn_statement(ctx *Return_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#while_statement.
	VisitWhile_statement(ctx *While_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_routine_statement.
	VisitSql_routine_statement(ctx *Sql_routine_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#common_table_expression.
	VisitCommon_table_expression(ctx *Common_table_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_alias_statement.
	VisitCreate_alias_statement(ctx *Create_alias_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_alias.
	VisitTable_alias(ctx *Table_aliasContext) interface{}

	// Visit a parse tree produced by Db2Parser#module_alias.
	VisitModule_alias(ctx *Module_aliasContext) interface{}

	// Visit a parse tree produced by Db2Parser#sequence_alias.
	VisitSequence_alias(ctx *Sequence_aliasContext) interface{}

	// Visit a parse tree produced by Db2Parser#or_replace.
	VisitOr_replace(ctx *Or_replaceContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_audit_policy_statement.
	VisitCreate_audit_policy_statement(ctx *Create_audit_policy_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#audit_policy_opts.
	VisitAudit_policy_opts(ctx *Audit_policy_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#audit_policy_categories_opts.
	VisitAudit_policy_categories_opts(ctx *Audit_policy_categories_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_bufferpool_statement.
	VisitCreate_bufferpool_statement(ctx *Create_bufferpool_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#bufferpool_opts.
	VisitBufferpool_opts(ctx *Bufferpool_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#except_clause.
	VisitExcept_clause(ctx *Except_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#member_list.
	VisitMember_list(ctx *Member_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#member_list_item.
	VisitMember_list_item(ctx *Member_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_database_partition_group_statement.
	VisitCreate_database_partition_group_statement(ctx *Create_database_partition_group_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_event_monitor_statement.
	VisitCreate_event_monitor_statement(ctx *Create_event_monitor_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_event_monitor_activities_statement.
	VisitCreate_event_monitor_activities_statement(ctx *Create_event_monitor_activities_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#formatted_event_table_info_3.
	VisitFormatted_event_table_info_3(ctx *Formatted_event_table_info_3Context) interface{}

	// Visit a parse tree produced by Db2Parser#create_event_monitor_change_history_statement.
	VisitCreate_event_monitor_change_history_statement(ctx *Create_event_monitor_change_history_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#event_control_list.
	VisitEvent_control_list(ctx *Event_control_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#event_control.
	VisitEvent_control(ctx *Event_controlContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_event_monitor_locking_statement.
	VisitCreate_event_monitor_locking_statement(ctx *Create_event_monitor_locking_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_event_monitor_package_cache_statement.
	VisitCreate_event_monitor_package_cache_statement(ctx *Create_event_monitor_package_cache_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#filter_and_collection_options.
	VisitFilter_and_collection_options(ctx *Filter_and_collection_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#event_condition.
	VisitEvent_condition(ctx *Event_conditionContext) interface{}

	// Visit a parse tree produced by Db2Parser#event_condition_item.
	VisitEvent_condition_item(ctx *Event_condition_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_event_monitor_statistics_statement.
	VisitCreate_event_monitor_statistics_statement(ctx *Create_event_monitor_statistics_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#event_monitor_statistics_opts.
	VisitEvent_monitor_statistics_opts(ctx *Event_monitor_statistics_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_event_monitor_threshold_violations_statement.
	VisitCreate_event_monitor_threshold_violations_statement(ctx *Create_event_monitor_threshold_violations_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#formatted_event_table_info_2.
	VisitFormatted_event_table_info_2(ctx *Formatted_event_table_info_2Context) interface{}

	// Visit a parse tree produced by Db2Parser#file_options.
	VisitFile_options(ctx *File_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#event_monitor_threshold_opts.
	VisitEvent_monitor_threshold_opts(ctx *Event_monitor_threshold_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#pages.
	VisitPages(ctx *PagesContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_event_monitor_unit_of_work.
	VisitCreate_event_monitor_unit_of_work(ctx *Create_event_monitor_unit_of_workContext) interface{}

	// Visit a parse tree produced by Db2Parser#formatted_event_table_info.
	VisitFormatted_event_table_info(ctx *Formatted_event_table_infoContext) interface{}

	// Visit a parse tree produced by Db2Parser#autostart_manualstart.
	VisitAutostart_manualstart(ctx *Autostart_manualstartContext) interface{}

	// Visit a parse tree produced by Db2Parser#evm_group.
	VisitEvm_group(ctx *Evm_groupContext) interface{}

	// Visit a parse tree produced by Db2Parser#target_table_options.
	VisitTarget_table_options(ctx *Target_table_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_external_table_statement.
	VisitCreate_external_table_statement(ctx *Create_external_table_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#ext_table_option.
	VisitExt_table_option(ctx *Ext_table_optionContext) interface{}

	// Visit a parse tree produced by Db2Parser#ext_table_option_value.
	VisitExt_table_option_value(ctx *Ext_table_option_valueContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_function_statement.
	VisitCreate_function_statement(ctx *Create_function_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_function_aggregate_interface_statement.
	VisitCreate_function_aggregate_interface_statement(ctx *Create_function_aggregate_interface_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#agg_fn_param_decl.
	VisitAgg_fn_param_decl(ctx *Agg_fn_param_declContext) interface{}

	// Visit a parse tree produced by Db2Parser#agg_fn_option_list.
	VisitAgg_fn_option_list(ctx *Agg_fn_option_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#state_variable_declaration.
	VisitState_variable_declaration(ctx *State_variable_declarationContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_function_external_scalar_statement.
	VisitCreate_function_external_scalar_statement(ctx *Create_function_external_scalar_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#ext_scalar_param_decl.
	VisitExt_scalar_param_decl(ctx *Ext_scalar_param_declContext) interface{}

	// Visit a parse tree produced by Db2Parser#ext_scalar_option_list.
	VisitExt_scalar_option_list(ctx *Ext_scalar_option_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#ext_scalar_option_list_item.
	VisitExt_scalar_option_list_item(ctx *Ext_scalar_option_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#predicate_specification.
	VisitPredicate_specification(ctx *Predicate_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_filter.
	VisitData_filter(ctx *Data_filterContext) interface{}

	// Visit a parse tree produced by Db2Parser#index_exploitation.
	VisitIndex_exploitation(ctx *Index_exploitationContext) interface{}

	// Visit a parse tree produced by Db2Parser#exploitation_rule.
	VisitExploitation_rule(ctx *Exploitation_ruleContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_function_external_table_statement.
	VisitCreate_function_external_table_statement(ctx *Create_function_external_table_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#ext_table_param_decl_list.
	VisitExt_table_param_decl_list(ctx *Ext_table_param_decl_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#ext_table_param_decl.
	VisitExt_table_param_decl(ctx *Ext_table_param_declContext) interface{}

	// Visit a parse tree produced by Db2Parser#ext_table_option_list.
	VisitExt_table_option_list(ctx *Ext_table_option_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#ext_table_option_list_item.
	VisitExt_table_option_list_item(ctx *Ext_table_option_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_function_old_db_external_function_statement.
	VisitCreate_function_old_db_external_function_statement(ctx *Create_function_old_db_external_function_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#oledb_option_list.
	VisitOledb_option_list(ctx *Oledb_option_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#oledb_option_list_item.
	VisitOledb_option_list_item(ctx *Oledb_option_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_function_sourced_or_template_statement.
	VisitCreate_function_sourced_or_template_statement(ctx *Create_function_sourced_or_template_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#fn_return_opts.
	VisitFn_return_opts(ctx *Fn_return_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#fn_return_opts_item.
	VisitFn_return_opts_item(ctx *Fn_return_opts_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#template_opts.
	VisitTemplate_opts(ctx *Template_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#template_opts_item.
	VisitTemplate_opts_item(ctx *Template_opts_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#ascii_unicode.
	VisitAscii_unicode(ctx *Ascii_unicodeContext) interface{}

	// Visit a parse tree produced by Db2Parser#param_decl_list_3.
	VisitParam_decl_list_3(ctx *Param_decl_list_3Context) interface{}

	// Visit a parse tree produced by Db2Parser#param_decl_3.
	VisitParam_decl_3(ctx *Param_decl_3Context) interface{}

	// Visit a parse tree produced by Db2Parser#create_function_sql_scalar_table_or_row_statement.
	VisitCreate_function_sql_scalar_table_or_row_statement(ctx *Create_function_sql_scalar_table_or_row_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#param_decl_list_2.
	VisitParam_decl_list_2(ctx *Param_decl_list_2Context) interface{}

	// Visit a parse tree produced by Db2Parser#param_decl_2.
	VisitParam_decl_2(ctx *Param_decl_2Context) interface{}

	// Visit a parse tree produced by Db2Parser#sql_function_body.
	VisitSql_function_body(ctx *Sql_function_bodyContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_function_mapping_statement.
	VisitCreate_function_mapping_statement(ctx *Create_function_mapping_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#function_options.
	VisitFunction_options(ctx *Function_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#function_option_name.
	VisitFunction_option_name(ctx *Function_option_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_global_temporary_table_statement.
	VisitCreate_global_temporary_table_statement(ctx *Create_global_temporary_table_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_global_temporary_table_opts.
	VisitCreate_global_temporary_table_opts(ctx *Create_global_temporary_table_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_global_temporary_table_item.
	VisitCreate_global_temporary_table_item(ctx *Create_global_temporary_table_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#delete_preserve.
	VisitDelete_preserve(ctx *Delete_preserveContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_histogram_template_statement.
	VisitCreate_histogram_template_statement(ctx *Create_histogram_template_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_index_statement.
	VisitCreate_index_statement(ctx *Create_index_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#index_col_opts.
	VisitIndex_col_opts(ctx *Index_col_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#index_col_opts_item.
	VisitIndex_col_opts_item(ctx *Index_col_opts_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#key_expression.
	VisitKey_expression(ctx *Key_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_index_extension_statement.
	VisitCreate_index_extension_statement(ctx *Create_index_extension_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#param_list.
	VisitParam_list(ctx *Param_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#index_maintenance.
	VisitIndex_maintenance(ctx *Index_maintenanceContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_function_invocation.
	VisitTable_function_invocation(ctx *Table_function_invocationContext) interface{}

	// Visit a parse tree produced by Db2Parser#index_search.
	VisitIndex_search(ctx *Index_searchContext) interface{}

	// Visit a parse tree produced by Db2Parser#search_method_definition.
	VisitSearch_method_definition(ctx *Search_method_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_mask_statement.
	VisitCreate_mask_statement(ctx *Create_mask_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#case_expression.
	VisitCase_expression(ctx *Case_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#range_producing_funciton_invocation.
	VisitRange_producing_funciton_invocation(ctx *Range_producing_funciton_invocationContext) interface{}

	// Visit a parse tree produced by Db2Parser#index_filtering_function_invocation.
	VisitIndex_filtering_function_invocation(ctx *Index_filtering_function_invocationContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_method_statement.
	VisitCreate_method_statement(ctx *Create_method_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_opts.
	VisitMethod_opts(ctx *Method_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_opts_item.
	VisitMethod_opts_item(ctx *Method_opts_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_signature.
	VisitMethod_signature(ctx *Method_signatureContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_param_list.
	VisitMethod_param_list(ctx *Method_param_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_type_3.
	VisitData_type_3(ctx *Data_type_3Context) interface{}

	// Visit a parse tree produced by Db2Parser#data_type_4.
	VisitData_type_4(ctx *Data_type_4Context) interface{}

	// Visit a parse tree produced by Db2Parser#sql_method_body.
	VisitSql_method_body(ctx *Sql_method_bodyContext) interface{}

	// Visit a parse tree produced by Db2Parser#compound_sql_inlined.
	VisitCompound_sql_inlined(ctx *Compound_sql_inlinedContext) interface{}

	// Visit a parse tree produced by Db2Parser#declare_variable_statement.
	VisitDeclare_variable_statement(ctx *Declare_variable_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#assignment_statement.
	VisitAssignment_statement(ctx *Assignment_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#compound_body_statement.
	VisitCompound_body_statement(ctx *Compound_body_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_statement_inlined.
	VisitSql_statement_inlined(ctx *Sql_statement_inlinedContext) interface{}

	// Visit a parse tree produced by Db2Parser#compound_sql_compiled.
	VisitCompound_sql_compiled(ctx *Compound_sql_compiledContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_statement_compiled.
	VisitSql_statement_compiled(ctx *Sql_statement_compiledContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_module_statement.
	VisitCreate_module_statement(ctx *Create_module_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_nickname_statement.
	VisitCreate_nickname_statement(ctx *Create_nickname_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#nick_name_option_name.
	VisitNick_name_option_name(ctx *Nick_name_option_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#remote_object_name.
	VisitRemote_object_name(ctx *Remote_object_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#non_relational_data_definition.
	VisitNon_relational_data_definition(ctx *Non_relational_data_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#nick_name_column_list.
	VisitNick_name_column_list(ctx *Nick_name_column_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#nick_name_column_list_item.
	VisitNick_name_column_list_item(ctx *Nick_name_column_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#nick_name_column_definition.
	VisitNick_name_column_definition(ctx *Nick_name_column_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#nick_name_column_options.
	VisitNick_name_column_options(ctx *Nick_name_column_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#federated_column_options.
	VisitFederated_column_options(ctx *Federated_column_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#column_option_name.
	VisitColumn_option_name(ctx *Column_option_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_permission_statement.
	VisitCreate_permission_statement(ctx *Create_permission_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_procedure_statement.
	VisitCreate_procedure_statement(ctx *Create_procedure_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_procedure_external_statement.
	VisitCreate_procedure_external_statement(ctx *Create_procedure_external_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#proc_ext_param_list.
	VisitProc_ext_param_list(ctx *Proc_ext_param_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#proc_ext_param.
	VisitProc_ext_param(ctx *Proc_ext_paramContext) interface{}

	// Visit a parse tree produced by Db2Parser#option_list_2.
	VisitOption_list_2(ctx *Option_list_2Context) interface{}

	// Visit a parse tree produced by Db2Parser#option_list_2_item.
	VisitOption_list_2_item(ctx *Option_list_2_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_procedure_sourced_statement.
	VisitCreate_procedure_sourced_statement(ctx *Create_procedure_sourced_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#source_procedure_clause.
	VisitSource_procedure_clause(ctx *Source_procedure_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#source_object_name.
	VisitSource_object_name(ctx *Source_object_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#option_list_1.
	VisitOption_list_1(ctx *Option_list_1Context) interface{}

	// Visit a parse tree produced by Db2Parser#option_list_1_item.
	VisitOption_list_1_item(ctx *Option_list_1_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#result_set_element_number.
	VisitResult_set_element_number(ctx *Result_set_element_numberContext) interface{}

	// Visit a parse tree produced by Db2Parser#unique_id.
	VisitUnique_id(ctx *Unique_idContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_procedure_sql_statement.
	VisitCreate_procedure_sql_statement(ctx *Create_procedure_sql_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#proc_parameter_list.
	VisitProc_parameter_list(ctx *Proc_parameter_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#proc_parameter_list_item.
	VisitProc_parameter_list_item(ctx *Proc_parameter_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#in_out_inout.
	VisitIn_out_inout(ctx *In_out_inoutContext) interface{}

	// Visit a parse tree produced by Db2Parser#option_list.
	VisitOption_list(ctx *Option_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#option_list_item.
	VisitOption_list_item(ctx *Option_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_procedure_body.
	VisitSql_procedure_body(ctx *Sql_procedure_bodyContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_role_statement.
	VisitCreate_role_statement(ctx *Create_role_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_schema_statement.
	VisitCreate_schema_statement(ctx *Create_schema_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#schema_sql_statement.
	VisitSchema_sql_statement(ctx *Schema_sql_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_security_label_component_statement.
	VisitCreate_security_label_component_statement(ctx *Create_security_label_component_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#array_clause.
	VisitArray_clause(ctx *Array_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#set_clause.
	VisitSet_clause(ctx *Set_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#tree_clause.
	VisitTree_clause(ctx *Tree_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#tree_clause_item.
	VisitTree_clause_item(ctx *Tree_clause_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_security_label_statement.
	VisitCreate_security_label_statement(ctx *Create_security_label_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_security_label_item.
	VisitCreate_security_label_item(ctx *Create_security_label_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_security_policy_statement.
	VisitCreate_security_policy_statement(ctx *Create_security_policy_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_sequence_statement.
	VisitCreate_sequence_statement(ctx *Create_sequence_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_sequence_opts.
	VisitCreate_sequence_opts(ctx *Create_sequence_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_sequence_opts_item.
	VisitCreate_sequence_opts_item(ctx *Create_sequence_opts_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_service_class_statement.
	VisitCreate_service_class_statement(ctx *Create_service_class_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#high_medium_low.
	VisitHigh_medium_low(ctx *High_medium_lowContext) interface{}

	// Visit a parse tree produced by Db2Parser#on_off.
	VisitOn_off(ctx *On_offContext) interface{}

	// Visit a parse tree produced by Db2Parser#soft_hard.
	VisitSoft_hard(ctx *Soft_hardContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_server_statement.
	VisitCreate_server_statement(ctx *Create_server_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#password_.
	VisitPassword_(ctx *Password_Context) interface{}

	// Visit a parse tree produced by Db2Parser#create_stogroup_statement.
	VisitCreate_stogroup_statement(ctx *Create_stogroup_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_stogroup_opts.
	VisitCreate_stogroup_opts(ctx *Create_stogroup_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_synonym_statement.
	VisitCreate_synonym_statement(ctx *Create_synonym_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_table_statement.
	VisitCreate_table_statement(ctx *Create_table_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_table_opts.
	VisitCreate_table_opts(ctx *Create_table_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_option_list.
	VisitTable_option_list(ctx *Table_option_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_option_list_item.
	VisitTable_option_list_item(ctx *Table_option_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_option_name.
	VisitTable_option_name(ctx *Table_option_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#element_list.
	VisitElement_list(ctx *Element_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#element_list_item.
	VisitElement_list_item(ctx *Element_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#column_definition.
	VisitColumn_definition(ctx *Column_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#period_definition.
	VisitPeriod_definition(ctx *Period_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#unique_constraint.
	VisitUnique_constraint(ctx *Unique_constraintContext) interface{}

	// Visit a parse tree produced by Db2Parser#referential_constraint.
	VisitReferential_constraint(ctx *Referential_constraintContext) interface{}

	// Visit a parse tree produced by Db2Parser#check_constraint.
	VisitCheck_constraint(ctx *Check_constraintContext) interface{}

	// Visit a parse tree produced by Db2Parser#column_options.
	VisitColumn_options(ctx *Column_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#column_options_item.
	VisitColumn_options_item(ctx *Column_options_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#references_clause.
	VisitReferences_clause(ctx *References_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#rule_clause.
	VisitRule_clause(ctx *Rule_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#constraint_attributes.
	VisitConstraint_attributes(ctx *Constraint_attributesContext) interface{}

	// Visit a parse tree produced by Db2Parser#default_clause.
	VisitDefault_clause(ctx *Default_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#default_values.
	VisitDefault_values(ctx *Default_valuesContext) interface{}

	// Visit a parse tree produced by Db2Parser#generated_clause.
	VisitGenerated_clause(ctx *Generated_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#datetime_special_register.
	VisitDatetime_special_register(ctx *Datetime_special_registerContext) interface{}

	// Visit a parse tree produced by Db2Parser#user_special_register.
	VisitUser_special_register(ctx *User_special_registerContext) interface{}

	// Visit a parse tree produced by Db2Parser#cast_function.
	VisitCast_function(ctx *Cast_functionContext) interface{}

	// Visit a parse tree produced by Db2Parser#identity_options.
	VisitIdentity_options(ctx *Identity_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#identity_options_item.
	VisitIdentity_options_item(ctx *Identity_options_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#as_row_change_timestamp_clause.
	VisitAs_row_change_timestamp_clause(ctx *As_row_change_timestamp_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#as_generated_expression_clause.
	VisitAs_generated_expression_clause(ctx *As_generated_expression_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#generation_expression.
	VisitGeneration_expression(ctx *Generation_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#as_row_transaction_timestamp_clause.
	VisitAs_row_transaction_timestamp_clause(ctx *As_row_transaction_timestamp_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#as_row_transaction_start_id_clause.
	VisitAs_row_transaction_start_id_clause(ctx *As_row_transaction_start_id_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#oid_column_definition.
	VisitOid_column_definition(ctx *Oid_column_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#range_partition_spec.
	VisitRange_partition_spec(ctx *Range_partition_specContext) interface{}

	// Visit a parse tree produced by Db2Parser#partition_expression_list.
	VisitPartition_expression_list(ctx *Partition_expression_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#partition_expression.
	VisitPartition_expression(ctx *Partition_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#partition_element_list.
	VisitPartition_element_list(ctx *Partition_element_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#partition_element.
	VisitPartition_element(ctx *Partition_elementContext) interface{}

	// Visit a parse tree produced by Db2Parser#boundary_spec.
	VisitBoundary_spec(ctx *Boundary_specContext) interface{}

	// Visit a parse tree produced by Db2Parser#partition_tablespace_options.
	VisitPartition_tablespace_options(ctx *Partition_tablespace_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#duration_label.
	VisitDuration_label(ctx *Duration_labelContext) interface{}

	// Visit a parse tree produced by Db2Parser#starting_clause.
	VisitStarting_clause(ctx *Starting_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#const_min_max_list.
	VisitConst_min_max_list(ctx *Const_min_max_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#const_min_max.
	VisitConst_min_max(ctx *Const_min_maxContext) interface{}

	// Visit a parse tree produced by Db2Parser#ending_clause.
	VisitEnding_clause(ctx *Ending_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#typed_table_options.
	VisitTyped_table_options(ctx *Typed_table_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#typed_element_list.
	VisitTyped_element_list(ctx *Typed_element_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#typed_element_list_item.
	VisitTyped_element_list_item(ctx *Typed_element_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#as_result_table.
	VisitAs_result_table(ctx *As_result_tableContext) interface{}

	// Visit a parse tree produced by Db2Parser#copy_options.
	VisitCopy_options(ctx *Copy_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#materialized_query_options.
	VisitMaterialized_query_options(ctx *Materialized_query_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#staging_table_definition.
	VisitStaging_table_definition(ctx *Staging_table_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#dimensions_clause.
	VisitDimensions_clause(ctx *Dimensions_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#col_names.
	VisitCol_names(ctx *Col_namesContext) interface{}

	// Visit a parse tree produced by Db2Parser#sequence_key_spec.
	VisitSequence_key_spec(ctx *Sequence_key_specContext) interface{}

	// Visit a parse tree produced by Db2Parser#sequence_key_spec_list.
	VisitSequence_key_spec_list(ctx *Sequence_key_spec_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#sequence_key_spec_list_item.
	VisitSequence_key_spec_list_item(ctx *Sequence_key_spec_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#tablespace_clauses.
	VisitTablespace_clauses(ctx *Tablespace_clausesContext) interface{}

	// Visit a parse tree produced by Db2Parser#distribution_clause.
	VisitDistribution_clause(ctx *Distribution_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#partitioning_clause.
	VisitPartitioning_clause(ctx *Partitioning_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#if_not_exists.
	VisitIf_not_exists(ctx *If_not_existsContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_tablespace_statement.
	VisitCreate_tablespace_statement(ctx *Create_tablespace_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#storage_group.
	VisitStorage_group(ctx *Storage_groupContext) interface{}

	// Visit a parse tree produced by Db2Parser#size_attributes.
	VisitSize_attributes(ctx *Size_attributesContext) interface{}

	// Visit a parse tree produced by Db2Parser#system_containers.
	VisitSystem_containers(ctx *System_containersContext) interface{}

	// Visit a parse tree produced by Db2Parser#container_string_list.
	VisitContainer_string_list(ctx *Container_string_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#database_containers.
	VisitDatabase_containers(ctx *Database_containersContext) interface{}

	// Visit a parse tree produced by Db2Parser#container_clause.
	VisitContainer_clause(ctx *Container_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#container_clause_list.
	VisitContainer_clause_list(ctx *Container_clause_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#container_clause_list_item.
	VisitContainer_clause_list_item(ctx *Container_clause_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#on_db_partitions_clause.
	VisitOn_db_partitions_clause(ctx *On_db_partitions_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_partition_number_list.
	VisitDb_partition_number_list(ctx *Db_partition_number_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_partition_number_list_item.
	VisitDb_partition_number_list_item(ctx *Db_partition_number_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_partition_number.
	VisitDb_partition_number(ctx *Db_partition_numberContext) interface{}

	// Visit a parse tree produced by Db2Parser#number_of_pages.
	VisitNumber_of_pages(ctx *Number_of_pagesContext) interface{}

	// Visit a parse tree produced by Db2Parser#number_of_files.
	VisitNumber_of_files(ctx *Number_of_filesContext) interface{}

	// Visit a parse tree produced by Db2Parser#number_of_milliseconds.
	VisitNumber_of_milliseconds(ctx *Number_of_millisecondsContext) interface{}

	// Visit a parse tree produced by Db2Parser#number_megabytes_per_second.
	VisitNumber_megabytes_per_second(ctx *Number_megabytes_per_secondContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_threshold_statement.
	VisitCreate_threshold_statement(ctx *Create_threshold_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#threshold_domain.
	VisitThreshold_domain(ctx *Threshold_domainContext) interface{}

	// Visit a parse tree produced by Db2Parser#statement_text.
	VisitStatement_text(ctx *Statement_textContext) interface{}

	// Visit a parse tree produced by Db2Parser#executable_id.
	VisitExecutable_id(ctx *Executable_idContext) interface{}

	// Visit a parse tree produced by Db2Parser#enforcement_scope.
	VisitEnforcement_scope(ctx *Enforcement_scopeContext) interface{}

	// Visit a parse tree produced by Db2Parser#threshold_predicate.
	VisitThreshold_predicate(ctx *Threshold_predicateContext) interface{}

	// Visit a parse tree produced by Db2Parser#checking_every.
	VisitChecking_every(ctx *Checking_everyContext) interface{}

	// Visit a parse tree produced by Db2Parser#hour_to_seconds.
	VisitHour_to_seconds(ctx *Hour_to_secondsContext) interface{}

	// Visit a parse tree produced by Db2Parser#day_to_minutes.
	VisitDay_to_minutes(ctx *Day_to_minutesContext) interface{}

	// Visit a parse tree produced by Db2Parser#day_to_seconds.
	VisitDay_to_seconds(ctx *Day_to_secondsContext) interface{}

	// Visit a parse tree produced by Db2Parser#threshold_exceeded_actions_2.
	VisitThreshold_exceeded_actions_2(ctx *Threshold_exceeded_actions_2Context) interface{}

	// Visit a parse tree produced by Db2Parser#details_section.
	VisitDetails_section(ctx *Details_sectionContext) interface{}

	// Visit a parse tree produced by Db2Parser#remap_activity_action.
	VisitRemap_activity_action(ctx *Remap_activity_actionContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_transform_statement.
	VisitCreate_transform_statement(ctx *Create_transform_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#tranform_list.
	VisitTranform_list(ctx *Tranform_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#tranform_list_item.
	VisitTranform_list_item(ctx *Tranform_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#transform_group_list.
	VisitTransform_group_list(ctx *Transform_group_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#transform_group_list_item.
	VisitTransform_group_list_item(ctx *Transform_group_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_trigger_statement.
	VisitCreate_trigger_statement(ctx *Create_trigger_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#ref_list.
	VisitRef_list(ctx *Ref_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#ref_list_item.
	VisitRef_list_item(ctx *Ref_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#old_new.
	VisitOld_new(ctx *Old_newContext) interface{}

	// Visit a parse tree produced by Db2Parser#correlation_name.
	VisitCorrelation_name(ctx *Correlation_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#identifier.
	VisitIdentifier(ctx *IdentifierContext) interface{}

	// Visit a parse tree produced by Db2Parser#trigger_event.
	VisitTrigger_event(ctx *Trigger_eventContext) interface{}

	// Visit a parse tree produced by Db2Parser#triggered_action.
	VisitTriggered_action(ctx *Triggered_actionContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_procedure_statement.
	VisitSql_procedure_statement(ctx *Sql_procedure_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_function_statement.
	VisitSql_function_statement(ctx *Sql_function_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_trusted_context_statement.
	VisitCreate_trusted_context_statement(ctx *Create_trusted_context_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#attr_list.
	VisitAttr_list(ctx *Attr_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#attr_list_item.
	VisitAttr_list_item(ctx *Attr_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#auth_list.
	VisitAuth_list(ctx *Auth_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#auth_list_item.
	VisitAuth_list_item(ctx *Auth_list_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#address_value.
	VisitAddress_value(ctx *Address_valueContext) interface{}

	// Visit a parse tree produced by Db2Parser#encryption_value.
	VisitEncryption_value(ctx *Encryption_valueContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_type_statement.
	VisitCreate_type_statement(ctx *Create_type_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_type_array_statement.
	VisitCreate_type_array_statement(ctx *Create_type_array_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_type_cursor_statement.
	VisitCreate_type_cursor_statement(ctx *Create_type_cursor_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_type_distinct_statement.
	VisitCreate_type_distinct_statement(ctx *Create_type_distinct_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_type_row_statement.
	VisitCreate_type_row_statement(ctx *Create_type_row_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#field_definition_list_paren.
	VisitField_definition_list_paren(ctx *Field_definition_list_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#field_definition_list.
	VisitField_definition_list(ctx *Field_definition_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#field_definition.
	VisitField_definition(ctx *Field_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_type_structured_statement.
	VisitCreate_type_structured_statement(ctx *Create_type_structured_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#structured_type_seq.
	VisitStructured_type_seq(ctx *Structured_type_seqContext) interface{}

	// Visit a parse tree produced by Db2Parser#attribute_definition_list_paren.
	VisitAttribute_definition_list_paren(ctx *Attribute_definition_list_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#attribute_definition_list.
	VisitAttribute_definition_list(ctx *Attribute_definition_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#attribute_definition.
	VisitAttribute_definition(ctx *Attribute_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_specification_list.
	VisitMethod_specification_list(ctx *Method_specification_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_specification.
	VisitMethod_specification(ctx *Method_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_specification_seq.
	VisitMethod_specification_seq(ctx *Method_specification_seqContext) interface{}

	// Visit a parse tree produced by Db2Parser#as_locator.
	VisitAs_locator(ctx *As_locatorContext) interface{}

	// Visit a parse tree produced by Db2Parser#param_decl_list_paren.
	VisitParam_decl_list_paren(ctx *Param_decl_list_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#param_decl_list.
	VisitParam_decl_list(ctx *Param_decl_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#param_decl.
	VisitParam_decl(ctx *Param_declContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_routine_characteristics.
	VisitSql_routine_characteristics(ctx *Sql_routine_characteristicsContext) interface{}

	// Visit a parse tree produced by Db2Parser#external_routine_characteristics.
	VisitExternal_routine_characteristics(ctx *External_routine_characteristicsContext) interface{}

	// Visit a parse tree produced by Db2Parser#length.
	VisitLength(ctx *LengthContext) interface{}

	// Visit a parse tree produced by Db2Parser#rep_type.
	VisitRep_type(ctx *Rep_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#varchars.
	VisitVarchars(ctx *VarcharsContext) interface{}

	// Visit a parse tree produced by Db2Parser#varbinaries.
	VisitVarbinaries(ctx *VarbinariesContext) interface{}

	// Visit a parse tree produced by Db2Parser#for_bit_data.
	VisitFor_bit_data(ctx *For_bit_dataContext) interface{}

	// Visit a parse tree produced by Db2Parser#lob_options.
	VisitLob_options(ctx *Lob_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_type_mapping_statement.
	VisitCreate_type_mapping_statement(ctx *Create_type_mapping_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#for_bit_data_precision.
	VisitFor_bit_data_precision(ctx *For_bit_data_precisionContext) interface{}

	// Visit a parse tree produced by Db2Parser#precision.
	VisitPrecision(ctx *PrecisionContext) interface{}

	// Visit a parse tree produced by Db2Parser#scale.
	VisitScale(ctx *ScaleContext) interface{}

	// Visit a parse tree produced by Db2Parser#precision_scale_comp.
	VisitPrecision_scale_comp(ctx *Precision_scale_compContext) interface{}

	// Visit a parse tree produced by Db2Parser#from_to.
	VisitFrom_to(ctx *From_toContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_source_data_type.
	VisitData_source_data_type(ctx *Data_source_data_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#local_data_type.
	VisitLocal_data_type(ctx *Local_data_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#remote_server.
	VisitRemote_server(ctx *Remote_serverContext) interface{}

	// Visit a parse tree produced by Db2Parser#server_version.
	VisitServer_version(ctx *Server_versionContext) interface{}

	// Visit a parse tree produced by Db2Parser#server_type.
	VisitServer_type(ctx *Server_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#version.
	VisitVersion(ctx *VersionContext) interface{}

	// Visit a parse tree produced by Db2Parser#release.
	VisitRelease(ctx *ReleaseContext) interface{}

	// Visit a parse tree produced by Db2Parser#mod.
	VisitMod(ctx *ModContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_usage_list_statement.
	VisitCreate_usage_list_statement(ctx *Create_usage_list_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_user_mapping_statement.
	VisitCreate_user_mapping_statement(ctx *Create_user_mapping_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#user_mapping_options_paren.
	VisitUser_mapping_options_paren(ctx *User_mapping_options_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#user_mapping_options.
	VisitUser_mapping_options(ctx *User_mapping_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_variable_statement.
	VisitCreate_variable_statement(ctx *Create_variable_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#constant_.
	VisitConstant_(ctx *Constant_Context) interface{}

	// Visit a parse tree produced by Db2Parser#special_register.
	VisitSpecial_register(ctx *Special_registerContext) interface{}

	// Visit a parse tree produced by Db2Parser#global_variable.
	VisitGlobal_variable(ctx *Global_variableContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_type_1.
	VisitData_type_1(ctx *Data_type_1Context) interface{}

	// Visit a parse tree produced by Db2Parser#cursor_value_constructor.
	VisitCursor_value_constructor(ctx *Cursor_value_constructorContext) interface{}

	// Visit a parse tree produced by Db2Parser#anchored_variable_data_type.
	VisitAnchored_variable_data_type(ctx *Anchored_variable_data_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#holdability.
	VisitHoldability(ctx *HoldabilityContext) interface{}

	// Visit a parse tree produced by Db2Parser#returnability.
	VisitReturnability(ctx *ReturnabilityContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_view_statement.
	VisitCreate_view_statement(ctx *Create_view_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_view_seq.
	VisitCreate_view_seq(ctx *Create_view_seqContext) interface{}

	// Visit a parse tree produced by Db2Parser#fullselect.
	VisitFullselect(ctx *FullselectContext) interface{}

	// Visit a parse tree produced by Db2Parser#subselect.
	VisitSubselect(ctx *SubselectContext) interface{}

	// Visit a parse tree produced by Db2Parser#select_clause.
	VisitSelect_clause(ctx *Select_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#select_clause_item.
	VisitSelect_clause_item(ctx *Select_clause_itemContext) interface{}

	// Visit a parse tree produced by Db2Parser#from_clause.
	VisitFrom_clause(ctx *From_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_reference.
	VisitTable_reference(ctx *Table_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_reference_list.
	VisitTable_reference_list(ctx *Table_reference_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#singles_table_reference.
	VisitSingles_table_reference(ctx *Singles_table_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#period_specification.
	VisitPeriod_specification(ctx *Period_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#value.
	VisitValue(ctx *ValueContext) interface{}

	// Visit a parse tree produced by Db2Parser#correlation_clause.
	VisitCorrelation_clause(ctx *Correlation_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#tablesample_clause.
	VisitTablesample_clause(ctx *Tablesample_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#numeric_expression.
	VisitNumeric_expression(ctx *Numeric_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#single_view_reference.
	VisitSingle_view_reference(ctx *Single_view_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#single_nickname_reference.
	VisitSingle_nickname_reference(ctx *Single_nickname_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#only_table_reference.
	VisitOnly_table_reference(ctx *Only_table_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#outer_table_reference.
	VisitOuter_table_reference(ctx *Outer_table_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#analyze_table_reference.
	VisitAnalyze_table_reference(ctx *Analyze_table_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#implementation_clause.
	VisitImplementation_clause(ctx *Implementation_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#nested_table_reference.
	VisitNested_table_reference(ctx *Nested_table_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#continue_handler.
	VisitContinue_handler(ctx *Continue_handlerContext) interface{}

	// Visit a parse tree produced by Db2Parser#specific_condition_value.
	VisitSpecific_condition_value(ctx *Specific_condition_valueContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_change_table_reference.
	VisitData_change_table_reference(ctx *Data_change_table_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#searched_update_statement.
	VisitSearched_update_statement(ctx *Searched_update_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#searched_delete_statement.
	VisitSearched_delete_statement(ctx *Searched_delete_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#final_new.
	VisitFinal_new(ctx *Final_newContext) interface{}

	// Visit a parse tree produced by Db2Parser#final_new_old.
	VisitFinal_new_old(ctx *Final_new_oldContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_function_reference.
	VisitTable_function_reference(ctx *Table_function_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_udf_cardinality_clause.
	VisitTable_udf_cardinality_clause(ctx *Table_udf_cardinality_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#typed_correlation_clause.
	VisitTyped_correlation_clause(ctx *Typed_correlation_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#column_name_data_type.
	VisitColumn_name_data_type(ctx *Column_name_data_typeContext) interface{}

	// Visit a parse tree produced by Db2Parser#collection_derived_table.
	VisitCollection_derived_table(ctx *Collection_derived_tableContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_function.
	VisitTable_function(ctx *Table_functionContext) interface{}

	// Visit a parse tree produced by Db2Parser#xmltable_expression.
	VisitXmltable_expression(ctx *Xmltable_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#xmltable_function.
	VisitXmltable_function(ctx *Xmltable_functionContext) interface{}

	// Visit a parse tree produced by Db2Parser#joined_table.
	VisitJoined_table(ctx *Joined_tableContext) interface{}

	// Visit a parse tree produced by Db2Parser#join_condition.
	VisitJoin_condition(ctx *Join_conditionContext) interface{}

	// Visit a parse tree produced by Db2Parser#outer.
	VisitOuter(ctx *OuterContext) interface{}

	// Visit a parse tree produced by Db2Parser#external_table_reference.
	VisitExternal_table_reference(ctx *External_table_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#column_definition_2.
	VisitColumn_definition_2(ctx *Column_definition_2Context) interface{}

	// Visit a parse tree produced by Db2Parser#file_name.
	VisitFile_name(ctx *File_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#where_clause.
	VisitWhere_clause(ctx *Where_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#group_by_clause.
	VisitGroup_by_clause(ctx *Group_by_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#group_by_clause_opts.
	VisitGroup_by_clause_opts(ctx *Group_by_clause_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#grouping_expression.
	VisitGrouping_expression(ctx *Grouping_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#grouping_sets.
	VisitGrouping_sets(ctx *Grouping_setsContext) interface{}

	// Visit a parse tree produced by Db2Parser#super_groups.
	VisitSuper_groups(ctx *Super_groupsContext) interface{}

	// Visit a parse tree produced by Db2Parser#grant_total.
	VisitGrant_total(ctx *Grant_totalContext) interface{}

	// Visit a parse tree produced by Db2Parser#having_clause.
	VisitHaving_clause(ctx *Having_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#order_by_clause.
	VisitOrder_by_clause(ctx *Order_by_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#order_by_clause_opts.
	VisitOrder_by_clause_opts(ctx *Order_by_clause_optsContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_designator.
	VisitTable_designator(ctx *Table_designatorContext) interface{}

	// Visit a parse tree produced by Db2Parser#asc_desc.
	VisitAsc_desc(ctx *Asc_descContext) interface{}

	// Visit a parse tree produced by Db2Parser#first_last.
	VisitFirst_last(ctx *First_lastContext) interface{}

	// Visit a parse tree produced by Db2Parser#sort_key.
	VisitSort_key(ctx *Sort_keyContext) interface{}

	// Visit a parse tree produced by Db2Parser#simple_column_name.
	VisitSimple_column_name(ctx *Simple_column_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#simple_integer.
	VisitSimple_integer(ctx *Simple_integerContext) interface{}

	// Visit a parse tree produced by Db2Parser#sork_key_expression.
	VisitSork_key_expression(ctx *Sork_key_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#offset_clause.
	VisitOffset_clause(ctx *Offset_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#offset_row_count.
	VisitOffset_row_count(ctx *Offset_row_countContext) interface{}

	// Visit a parse tree produced by Db2Parser#fetch_clause.
	VisitFetch_clause(ctx *Fetch_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#fetch_row_count.
	VisitFetch_row_count(ctx *Fetch_row_countContext) interface{}

	// Visit a parse tree produced by Db2Parser#row_rows.
	VisitRow_rows(ctx *Row_rowsContext) interface{}

	// Visit a parse tree produced by Db2Parser#isolation_clause.
	VisitIsolation_clause(ctx *Isolation_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#lock_request_clause.
	VisitLock_request_clause(ctx *Lock_request_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#values_clause.
	VisitValues_clause(ctx *Values_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#values_row.
	VisitValues_row(ctx *Values_rowContext) interface{}

	// Visit a parse tree produced by Db2Parser#root_view_definition.
	VisitRoot_view_definition(ctx *Root_view_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#subview_definition.
	VisitSubview_definition(ctx *Subview_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#oid_column.
	VisitOid_column(ctx *Oid_columnContext) interface{}

	// Visit a parse tree produced by Db2Parser#with_options.
	VisitWith_options(ctx *With_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#with_option_def.
	VisitWith_option_def(ctx *With_option_defContext) interface{}

	// Visit a parse tree produced by Db2Parser#with_option_scope_def.
	VisitWith_option_scope_def(ctx *With_option_scope_defContext) interface{}

	// Visit a parse tree produced by Db2Parser#under_clause.
	VisitUnder_clause(ctx *Under_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_work_action_set_statement.
	VisitCreate_work_action_set_statement(ctx *Create_work_action_set_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_action_definition_list_paren.
	VisitWork_action_definition_list_paren(ctx *Work_action_definition_list_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_action_definition_list.
	VisitWork_action_definition_list(ctx *Work_action_definition_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_action_definition.
	VisitWork_action_definition(ctx *Work_action_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#action_types_clause.
	VisitAction_types_clause(ctx *Action_types_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#threshold_types_clause.
	VisitThreshold_types_clause(ctx *Threshold_types_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#second_seconds.
	VisitSecond_seconds(ctx *Second_secondsContext) interface{}

	// Visit a parse tree produced by Db2Parser#hours_minutes.
	VisitHours_minutes(ctx *Hours_minutesContext) interface{}

	// Visit a parse tree produced by Db2Parser#threshold_exceeded_actions.
	VisitThreshold_exceeded_actions(ctx *Threshold_exceeded_actionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#collect_activity_data_clause.
	VisitCollect_activity_data_clause(ctx *Collect_activity_data_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#with_without.
	VisitWith_without(ctx *With_withoutContext) interface{}

	// Visit a parse tree produced by Db2Parser#histogram_templace_clause.
	VisitHistogram_templace_clause(ctx *Histogram_templace_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_work_class_set_statement.
	VisitCreate_work_class_set_statement(ctx *Create_work_class_set_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_class_definition_list_paren.
	VisitWork_class_definition_list_paren(ctx *Work_class_definition_list_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_class_definition_list.
	VisitWork_class_definition_list(ctx *Work_class_definition_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_class_definition.
	VisitWork_class_definition(ctx *Work_class_definitionContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_attributes.
	VisitWork_attributes(ctx *Work_attributesContext) interface{}

	// Visit a parse tree produced by Db2Parser#position_clause.
	VisitPosition_clause(ctx *Position_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#position_.
	VisitPosition_(ctx *Position_Context) interface{}

	// Visit a parse tree produced by Db2Parser#for_from_to_clause.
	VisitFor_from_to_clause(ctx *For_from_to_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#from_value.
	VisitFrom_value(ctx *From_valueContext) interface{}

	// Visit a parse tree produced by Db2Parser#to_value.
	VisitTo_value(ctx *To_valueContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_tag_clause.
	VisitData_tag_clause(ctx *Data_tag_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#schema_clause.
	VisitSchema_clause(ctx *Schema_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_workload_statement.
	VisitCreate_workload_statement(ctx *Create_workload_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#pkg_exec_seq.
	VisitPkg_exec_seq(ctx *Pkg_exec_seqContext) interface{}

	// Visit a parse tree produced by Db2Parser#position_clause_2.
	VisitPosition_clause_2(ctx *Position_clause_2Context) interface{}

	// Visit a parse tree produced by Db2Parser#connection_attributes.
	VisitConnection_attributes(ctx *Connection_attributesContext) interface{}

	// Visit a parse tree produced by Db2Parser#string_list.
	VisitString_list(ctx *String_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#string_list_paren.
	VisitString_list_paren(ctx *String_list_parenContext) interface{}

	// Visit a parse tree produced by Db2Parser#workload_attributes.
	VisitWorkload_attributes(ctx *Workload_attributesContext) interface{}

	// Visit a parse tree produced by Db2Parser#degree.
	VisitDegree(ctx *DegreeContext) interface{}

	// Visit a parse tree produced by Db2Parser#allow_disallow.
	VisitAllow_disallow(ctx *Allow_disallowContext) interface{}

	// Visit a parse tree produced by Db2Parser#collect_on_clause.
	VisitCollect_on_clause(ctx *Collect_on_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#collect_details_clause.
	VisitCollect_details_clause(ctx *Collect_details_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#collect_lock_wait_options.
	VisitCollect_lock_wait_options(ctx *Collect_lock_wait_optionsContext) interface{}

	// Visit a parse tree produced by Db2Parser#wait_time.
	VisitWait_time(ctx *Wait_timeContext) interface{}

	// Visit a parse tree produced by Db2Parser#create_wrapper_statement.
	VisitCreate_wrapper_statement(ctx *Create_wrapper_statementContext) interface{}

	// Visit a parse tree produced by Db2Parser#wrapper_option_list.
	VisitWrapper_option_list(ctx *Wrapper_option_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#wrapper_option.
	VisitWrapper_option(ctx *Wrapper_optionContext) interface{}

	// Visit a parse tree produced by Db2Parser#expression.
	VisitExpression(ctx *ExpressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#function_invocation.
	VisitFunction_invocation(ctx *Function_invocationContext) interface{}

	// Visit a parse tree produced by Db2Parser#all_distinct.
	VisitAll_distinct(ctx *All_distinctContext) interface{}

	// Visit a parse tree produced by Db2Parser#scalar_fullselect.
	VisitScalar_fullselect(ctx *Scalar_fullselectContext) interface{}

	// Visit a parse tree produced by Db2Parser#cast_specification.
	VisitCast_specification(ctx *Cast_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#cursor_cast_specification.
	VisitCursor_cast_specification(ctx *Cursor_cast_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#row_cast_specification.
	VisitRow_cast_specification(ctx *Row_cast_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#interval_cast_specification.
	VisitInterval_cast_specification(ctx *Interval_cast_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#xmlcast_specification.
	VisitXmlcast_specification(ctx *Xmlcast_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#array_element_specification.
	VisitArray_element_specification(ctx *Array_element_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#array_constructor.
	VisitArray_constructor(ctx *Array_constructorContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_invocation.
	VisitMethod_invocation(ctx *Method_invocationContext) interface{}

	// Visit a parse tree produced by Db2Parser#olap_specification.
	VisitOlap_specification(ctx *Olap_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#ordered_olap_specification.
	VisitOrdered_olap_specification(ctx *Ordered_olap_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#window_partition_clause.
	VisitWindow_partition_clause(ctx *Window_partition_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#window_order_clause.
	VisitWindow_order_clause(ctx *Window_order_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#numbering_specification.
	VisitNumbering_specification(ctx *Numbering_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#aggregation_specification.
	VisitAggregation_specification(ctx *Aggregation_specificationContext) interface{}

	// Visit a parse tree produced by Db2Parser#olap_aggregate_function.
	VisitOlap_aggregate_function(ctx *Olap_aggregate_functionContext) interface{}

	// Visit a parse tree produced by Db2Parser#first_value_function.
	VisitFirst_value_function(ctx *First_value_functionContext) interface{}

	// Visit a parse tree produced by Db2Parser#last_value_function.
	VisitLast_value_function(ctx *Last_value_functionContext) interface{}

	// Visit a parse tree produced by Db2Parser#nth_value_function.
	VisitNth_value_function(ctx *Nth_value_functionContext) interface{}

	// Visit a parse tree produced by Db2Parser#ratio_to_report_function.
	VisitRatio_to_report_function(ctx *Ratio_to_report_functionContext) interface{}

	// Visit a parse tree produced by Db2Parser#ignore_respect_nulls.
	VisitIgnore_respect_nulls(ctx *Ignore_respect_nullsContext) interface{}

	// Visit a parse tree produced by Db2Parser#from_first_last.
	VisitFrom_first_last(ctx *From_first_lastContext) interface{}

	// Visit a parse tree produced by Db2Parser#window_aggregation_group_clause.
	VisitWindow_aggregation_group_clause(ctx *Window_aggregation_group_clauseContext) interface{}

	// Visit a parse tree produced by Db2Parser#group_start.
	VisitGroup_start(ctx *Group_startContext) interface{}

	// Visit a parse tree produced by Db2Parser#group_between.
	VisitGroup_between(ctx *Group_betweenContext) interface{}

	// Visit a parse tree produced by Db2Parser#group_bound1.
	VisitGroup_bound1(ctx *Group_bound1Context) interface{}

	// Visit a parse tree produced by Db2Parser#group_bound2.
	VisitGroup_bound2(ctx *Group_bound2Context) interface{}

	// Visit a parse tree produced by Db2Parser#group_end.
	VisitGroup_end(ctx *Group_endContext) interface{}

	// Visit a parse tree produced by Db2Parser#row_change_expression.
	VisitRow_change_expression(ctx *Row_change_expressionContext) interface{}

	// Visit a parse tree produced by Db2Parser#sequence_reference.
	VisitSequence_reference(ctx *Sequence_referenceContext) interface{}

	// Visit a parse tree produced by Db2Parser#subtype_treatment.
	VisitSubtype_treatment(ctx *Subtype_treatmentContext) interface{}

	// Visit a parse tree produced by Db2Parser#expression_list.
	VisitExpression_list(ctx *Expression_listContext) interface{}

	// Visit a parse tree produced by Db2Parser#expression_list_in_parentheses.
	VisitExpression_list_in_parentheses(ctx *Expression_list_in_parenthesesContext) interface{}

	// Visit a parse tree produced by Db2Parser#id_.
	VisitId_(ctx *Id_Context) interface{}

	// Visit a parse tree produced by Db2Parser#exposed_name.
	VisitExposed_name(ctx *Exposed_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#name.
	VisitName(ctx *NameContext) interface{}

	// Visit a parse tree produced by Db2Parser#label.
	VisitLabel(ctx *LabelContext) interface{}

	// Visit a parse tree produced by Db2Parser#host_label.
	VisitHost_label(ctx *Host_labelContext) interface{}

	// Visit a parse tree produced by Db2Parser#library_name.
	VisitLibrary_name(ctx *Library_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#array_type_name.
	VisitArray_type_name(ctx *Array_type_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#attribute_name.
	VisitAttribute_name(ctx *Attribute_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#row_type_name.
	VisitRow_type_name(ctx *Row_type_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#authorization_name.
	VisitAuthorization_name(ctx *Authorization_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#boolean_variable_name.
	VisitBoolean_variable_name(ctx *Boolean_variable_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#array_variable_name.
	VisitArray_variable_name(ctx *Array_variable_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#column_name.
	VisitColumn_name(ctx *Column_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#constraint_name.
	VisitConstraint_name(ctx *Constraint_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#descriptor_name.
	VisitDescriptor_name(ctx *Descriptor_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#distinct_type_name.
	VisitDistinct_type_name(ctx *Distinct_type_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#cursor_name.
	VisitCursor_name(ctx *Cursor_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#cursor_type_name.
	VisitCursor_type_name(ctx *Cursor_type_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#condition_name.
	VisitCondition_name(ctx *Condition_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#data_source_name.
	VisitData_source_name(ctx *Data_source_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#expression_name.
	VisitExpression_name(ctx *Expression_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#group_name.
	VisitGroup_name(ctx *Group_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#policy_name.
	VisitPolicy_name(ctx *Policy_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#bufferpool_name.
	VisitBufferpool_name(ctx *Bufferpool_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_partition_name.
	VisitDb_partition_name(ctx *Db_partition_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#database_name.
	VisitDatabase_name(ctx *Database_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#event_monitor_name.
	VisitEvent_monitor_name(ctx *Event_monitor_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#field_name.
	VisitField_name(ctx *Field_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#for_loop_name.
	VisitFor_loop_name(ctx *For_loop_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#function_name.
	VisitFunction_name(ctx *Function_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#function_mapping_name.
	VisitFunction_mapping_name(ctx *Function_mapping_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#global_variable_name.
	VisitGlobal_variable_name(ctx *Global_variable_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#hierarchy_name.
	VisitHierarchy_name(ctx *Hierarchy_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#host_variable_name.
	VisitHost_variable_name(ctx *Host_variable_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#parameter_marker.
	VisitParameter_marker(ctx *Parameter_markerContext) interface{}

	// Visit a parse tree produced by Db2Parser#template_name.
	VisitTemplate_name(ctx *Template_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#index_name.
	VisitIndex_name(ctx *Index_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#index_extension_name.
	VisitIndex_extension_name(ctx *Index_extension_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#input_descriptor_name.
	VisitInput_descriptor_name(ctx *Input_descriptor_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#mask_name.
	VisitMask_name(ctx *Mask_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#method_name.
	VisitMethod_name(ctx *Method_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#model_name.
	VisitModel_name(ctx *Model_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#module_name.
	VisitModule_name(ctx *Module_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#new_owner.
	VisitNew_owner(ctx *New_ownerContext) interface{}

	// Visit a parse tree produced by Db2Parser#nick_name.
	VisitNick_name(ctx *Nick_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#object_name.
	VisitObject_name(ctx *Object_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#oid_column_name.
	VisitOid_column_name(ctx *Oid_column_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#optimization_profile_name.
	VisitOptimization_profile_name(ctx *Optimization_profile_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#package_name.
	VisitPackage_name(ctx *Package_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#partition_name.
	VisitPartition_name(ctx *Partition_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#path_name.
	VisitPath_name(ctx *Path_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#permission_name.
	VisitPermission_name(ctx *Permission_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#pipe_name.
	VisitPipe_name(ctx *Pipe_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#procedure_name.
	VisitProcedure_name(ctx *Procedure_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#result_descriptor_name.
	VisitResult_descriptor_name(ctx *Result_descriptor_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#role_name.
	VisitRole_name(ctx *Role_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#root_table_name.
	VisitRoot_table_name(ctx *Root_table_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#root_view_name.
	VisitRoot_view_name(ctx *Root_view_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#row_variable_name.
	VisitRow_variable_name(ctx *Row_variable_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#source_schema_name.
	VisitSource_schema_name(ctx *Source_schema_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#source_package_name.
	VisitSource_package_name(ctx *Source_package_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#source_procedure_name.
	VisitSource_procedure_name(ctx *Source_procedure_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_parameter_name.
	VisitSql_parameter_name(ctx *Sql_parameter_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#sql_variable_name.
	VisitSql_variable_name(ctx *Sql_variable_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#transition_variable_name.
	VisitTransition_variable_name(ctx *Transition_variable_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#savepoint_name.
	VisitSavepoint_name(ctx *Savepoint_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#specific_name.
	VisitSpecific_name(ctx *Specific_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#schema.
	VisitSchema(ctx *SchemaContext) interface{}

	// Visit a parse tree produced by Db2Parser#schema_name.
	VisitSchema_name(ctx *Schema_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#search_method_name.
	VisitSearch_method_name(ctx *Search_method_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#server_name.
	VisitServer_name(ctx *Server_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#server_option_name.
	VisitServer_option_name(ctx *Server_option_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#session_authorization_name.
	VisitSession_authorization_name(ctx *Session_authorization_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#component_name.
	VisitComponent_name(ctx *Component_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#sec_label_comp_name.
	VisitSec_label_comp_name(ctx *Sec_label_comp_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#security_policy_name.
	VisitSecurity_policy_name(ctx *Security_policy_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#security_label_name.
	VisitSecurity_label_name(ctx *Security_label_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#sequence_name.
	VisitSequence_name(ctx *Sequence_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#service_class_name.
	VisitService_class_name(ctx *Service_class_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#service_superclass_name.
	VisitService_superclass_name(ctx *Service_superclass_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#storagegroup_name.
	VisitStoragegroup_name(ctx *Storagegroup_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#supertype_name.
	VisitSupertype_name(ctx *Supertype_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#superview_name.
	VisitSuperview_name(ctx *Superview_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#service_subclass_name.
	VisitService_subclass_name(ctx *Service_subclass_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#statement_name.
	VisitStatement_name(ctx *Statement_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#table_name.
	VisitTable_name(ctx *Table_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#tablespace_name.
	VisitTablespace_name(ctx *Tablespace_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#target_identifier.
	VisitTarget_identifier(ctx *Target_identifierContext) interface{}

	// Visit a parse tree produced by Db2Parser#threshold_name.
	VisitThreshold_name(ctx *Threshold_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#trigger_name.
	VisitTrigger_name(ctx *Trigger_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#context_name.
	VisitContext_name(ctx *Context_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#usage_list_name.
	VisitUsage_list_name(ctx *Usage_list_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#type_name.
	VisitType_name(ctx *Type_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#type_mapping_name.
	VisitType_mapping_name(ctx *Type_mapping_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#typed_table_name.
	VisitTyped_table_name(ctx *Typed_table_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#typed_view_name.
	VisitTyped_view_name(ctx *Typed_view_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#user_mapping_option_name.
	VisitUser_mapping_option_name(ctx *User_mapping_option_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#view_name.
	VisitView_name(ctx *View_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#variable_name.
	VisitVariable_name(ctx *Variable_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_action_set_name.
	VisitWork_action_set_name(ctx *Work_action_set_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_class_set_name.
	VisitWork_class_set_name(ctx *Work_class_set_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#workload_name.
	VisitWorkload_name(ctx *Workload_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_action_name.
	VisitWork_action_name(ctx *Work_action_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#work_class_name.
	VisitWork_class_name(ctx *Work_class_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#wrapper_name.
	VisitWrapper_name(ctx *Wrapper_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#wrapper_option_name.
	VisitWrapper_option_name(ctx *Wrapper_option_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#xsrobject_name.
	VisitXsrobject_name(ctx *Xsrobject_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#parameter_name.
	VisitParameter_name(ctx *Parameter_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#cursor_variable_name.
	VisitCursor_variable_name(ctx *Cursor_variable_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#alias_name.
	VisitAlias_name(ctx *Alias_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#db_partition_group_name.
	VisitDb_partition_group_name(ctx *Db_partition_group_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#source_index_name.
	VisitSource_index_name(ctx *Source_index_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#source_table_name.
	VisitSource_table_name(ctx *Source_table_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#source_storagegroup_name.
	VisitSource_storagegroup_name(ctx *Source_storagegroup_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#target_storagegroup_name.
	VisitTarget_storagegroup_name(ctx *Target_storagegroup_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#source_tablespace_name.
	VisitSource_tablespace_name(ctx *Source_tablespace_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#target_tablespace_name.
	VisitTarget_tablespace_name(ctx *Target_tablespace_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#unqualified_function_name.
	VisitUnqualified_function_name(ctx *Unqualified_function_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#unqualified_procedure_name.
	VisitUnqualified_procedure_name(ctx *Unqualified_procedure_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#unqualified_specific_name.
	VisitUnqualified_specific_name(ctx *Unqualified_specific_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#period_name.
	VisitPeriod_name(ctx *Period_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#history_table_name.
	VisitHistory_table_name(ctx *History_table_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#xml_schema_name.
	VisitXml_schema_name(ctx *Xml_schema_nameContext) interface{}

	// Visit a parse tree produced by Db2Parser#todo.
	VisitTodo(ctx *TodoContext) interface{}
}
