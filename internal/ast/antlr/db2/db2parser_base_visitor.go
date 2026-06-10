// Code generated from Db2Parser.g4 by ANTLR 4.13.2. DO NOT EDIT.

package db2 // Db2Parser
import "github.com/antlr4-go/antlr/v4"

type BaseDb2ParserVisitor struct {
	*antlr.BaseParseTreeVisitor
}

func (v *BaseDb2ParserVisitor) VisitDb2_file(ctx *Db2_fileContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBatch(ctx *BatchContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_statement(ctx *Sql_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_schema_statement(ctx *Sql_schema_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_data_change_statement(ctx *Sql_data_change_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_data_statement(ctx *Sql_data_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_transaction_statement(ctx *Sql_transaction_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_connection_statement(ctx *Sql_connection_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_dynamic_statement(ctx *Sql_dynamic_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_session_statement(ctx *Sql_session_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_embedded_host_language_statement(ctx *Sql_embedded_host_language_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_constrol_statement(ctx *Sql_constrol_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSelect_statement(ctx *Select_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRead_only_clause(ctx *Read_only_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUpdate_clause(ctx *Update_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOptimize_for_clause(ctx *Optimize_for_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitConcurrent_access_resolution_clause(ctx *Concurrent_access_resolution_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDelete_statement(ctx *Delete_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDelete_statement_searched_delete(ctx *Delete_statement_searched_deleteContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_or_view_name(ctx *Table_or_view_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDelete_statement_positioned_delete(ctx *Delete_statement_positioned_deleteContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDelete_deltalake_statement(ctx *Delete_deltalake_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInsert_statement(ctx *Insert_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInsert_datalake_statement(ctx *Insert_datalake_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitValues_item(ctx *Values_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMerge_statement(ctx *Merge_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_view_fullselect(ctx *Table_view_fullselectContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCommon_table_expression_list(ctx *Common_table_expression_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMatching_condition(ctx *Matching_conditionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModification_operation(ctx *Modification_operationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUpdate_operation(ctx *Update_operationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDelete_operation(ctx *Delete_operationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInsert_operation(ctx *Insert_operationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExpr_null_default_list(ctx *Expr_null_default_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIsolation_level(ctx *Isolation_levelContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTruncate_statement(ctx *Truncate_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUpdate_statement(ctx *Update_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUpdate_statement_searched_update(ctx *Update_statement_searched_updateContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSkip_wait(ctx *Skip_waitContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUpdate_statement_positioned_update(ctx *Update_statement_positioned_updateContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInclude_columns(ctx *Include_columnsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAssignment_clause(ctx *Assignment_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAssignment_item(ctx *Assignment_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPeriod_clause(ctx *Period_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTime_sec(ctx *Time_secContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUpdate_datalake_statement(ctx *Update_datalake_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_database_authorities_statement(ctx *Grant_database_authorities_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_privilege_list(ctx *Db_privilege_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_privilege(ctx *Db_privilegeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrantee(ctx *GranteeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrantee_user_group(ctx *Grantee_user_groupContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUser_group(ctx *User_groupContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrantee_list(ctx *Grantee_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrantee_list_public(ctx *Grantee_list_publicContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrantee_list_user_group(ctx *Grantee_list_user_groupContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_exemption_statement(ctx *Grant_exemption_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExemption_privilege(ctx *Exemption_privilegeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_global_variable_privileges_statement(ctx *Grant_global_variable_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitVariable_privilege(ctx *Variable_privilegeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRead_write(ctx *Read_writeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWith_grant_option(ctx *With_grant_optionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_index_privileges_statement(ctx *Grant_index_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_module_privileges_statement(ctx *Grant_module_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_package_privileges_statement(ctx *Grant_package_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPackage_privilege_list(ctx *Package_privilege_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPackage_privilege(ctx *Package_privilegeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_role_statement(ctx *Grant_role_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRole_list(ctx *Role_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_routine_privileges_statement(ctx *Grant_routine_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_schema_privileges_statement(ctx *Grant_schema_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSchema_privilege_list(ctx *Schema_privilege_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSchema_privilege(ctx *Schema_privilegeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_security_label_statement(ctx *Grant_security_label_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_sequence_privileges_statement(ctx *Grant_sequence_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSequence_privilege_list(ctx *Sequence_privilege_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSequence_privilege(ctx *Sequence_privilegeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_server_privileges_statement(ctx *Grant_server_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_setsessionuser_privilege_statement(ctx *Grant_setsessionuser_privilege_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUser_list(ctx *User_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUser_auth(ctx *User_authContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_table_space_privileges_statement(ctx *Grant_table_space_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_table_view_or_nickname_privileges_statement(ctx *Grant_table_view_or_nickname_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTvn_privilege_list(ctx *Tvn_privilege_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTvn_privilege(ctx *Tvn_privilegeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitColumn_name_list_paren(ctx *Column_name_list_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitColumn_name_list(ctx *Column_name_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_workload_privileges_statement(ctx *Grant_workload_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_xsr_object_privileges_statement(ctx *Grant_xsr_object_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_database_authorities_statement(ctx *Revoke_database_authorities_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBy_all(ctx *By_allContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_exemption_statement(ctx *Revoke_exemption_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_global_variable_privileges_statement(ctx *Revoke_global_variable_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_index_privileges_statement(ctx *Revoke_index_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_module_privileges_statement(ctx *Revoke_module_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_package_privileges_statement(ctx *Revoke_package_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_role_statement(ctx *Revoke_role_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_routine_privileges_statement(ctx *Revoke_routine_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_schema_privileges_statement(ctx *Revoke_schema_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_security_label_statement(ctx *Revoke_security_label_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_sequence_privileges_statement(ctx *Revoke_sequence_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_server_privileges_statement(ctx *Revoke_server_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_setsessionuser_privilege_statement(ctx *Revoke_setsessionuser_privilege_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_table_space_privileges_statement(ctx *Revoke_table_space_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_table_view_or_nickname_privileges_statement(ctx *Revoke_table_view_or_nickname_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_workload_privileges_statement(ctx *Revoke_workload_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRevoke_xsr_object_privileges_statement(ctx *Revoke_xsr_object_privileges_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUser_group_role(ctx *User_group_roleContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRollback_statement(ctx *Rollback_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSavepoint_statement(ctx *Savepoint_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRelease_savepoint_statement(ctx *Release_savepoint_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAllocate_cursor_statement(ctx *Allocate_cursor_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_audit_policy_statement(ctx *Alter_audit_policy_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitStatus_spec(ctx *Status_specContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNormal_audit(ctx *Normal_auditContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_bufferpool_statement(ctx *Alter_bufferpool_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitImmediate_deferred(ctx *Immediate_deferredContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_database_partition_group_statement(ctx *Alter_database_partition_group_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_partition_group_list_item(ctx *Db_partition_group_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_partition_num_nums(ctx *Db_partition_num_numsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_partitions_clause(ctx *Db_partitions_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_partition_options(ctx *Db_partition_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_database_statement(ctx *Alter_database_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_database_opts(ctx *Alter_database_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_event_monitor_statement(ctx *Alter_event_monitor_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_event_monitor_opts(ctx *Alter_event_monitor_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_function_statement(ctx *Alter_function_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_function_opts(ctx *Alter_function_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFunction_designator(ctx *Function_designatorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_type_list(ctx *Data_type_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_type_list_paren(ctx *Data_type_list_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_histogram_template_statement(ctx *Alter_histogram_template_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_index_statement(ctx *Alter_index_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitYes_no(ctx *Yes_noContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_mask_statement(ctx *Alter_mask_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEnable_disable(ctx *Enable_disableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_method_statement(ctx *Alter_method_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_designator(ctx *Method_designatorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_model_statement(ctx *Alter_model_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_module_statement(ctx *Alter_module_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_module_opts(ctx *Alter_module_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModule_function_definition(ctx *Module_function_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModule_procedure_definition(ctx *Module_procedure_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModule_type_definition(ctx *Module_type_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModule_variable_definition(ctx *Module_variable_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModule_condition_definition(ctx *Module_condition_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModule_object_identification(ctx *Module_object_identificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModule_function_designator(ctx *Module_function_designatorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModule_procedure_designator(ctx *Module_procedure_designatorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_nickname_statement(ctx *Alter_nickname_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_nickname_opts_1(ctx *Alter_nickname_opts_1Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_nickname_opts_1_item(ctx *Alter_nickname_opts_1_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_nickname_opts_2(ctx *Alter_nickname_opts_2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_nickname_opts_2_item(ctx *Alter_nickname_opts_2_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitConstraint_alteration(ctx *Constraint_alterationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_package_statement(ctx *Alter_package_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_package_opts(ctx *Alter_package_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_permission_statement(ctx *Alter_permission_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_procedure_external_statement(ctx *Alter_procedure_external_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_procedure_external_opts(ctx *Alter_procedure_external_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitProcedure_designator(ctx *Procedure_designatorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_procedure_sourced_statement(ctx *Alter_procedure_sourced_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitParameter_alteration(ctx *Parameter_alterationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_procedure_sql_statement(ctx *Alter_procedure_sql_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_schema_statement(ctx *Alter_schema_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNone_changes(ctx *None_changesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_security_label_component_statement(ctx *Alter_security_label_component_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAdd_element_clause(ctx *Add_element_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitArray_element_clause(ctx *Array_element_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTree_element_clause(ctx *Tree_element_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_security_policy_statement(ctx *Alter_security_policy_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_security_policy_opts(ctx *Alter_security_policy_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_sequence_statement(ctx *Alter_sequence_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_sequence_opts(ctx *Alter_sequence_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_server_statement(ctx *Alter_server_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_server_opts(ctx *Alter_server_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_service_class_statement(ctx *Alter_service_class_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_service_class_opts(ctx *Alter_service_class_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDefault_on_off(ctx *Default_on_offContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDefault_high_medium_low(ctx *Default_high_medium_lowContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_stogroup_statement(ctx *Alter_stogroup_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_stogroup_opts(ctx *Alter_stogroup_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_table_statement(ctx *Alter_table_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_table_opts(ctx *Alter_table_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNull_on_off(ctx *Null_on_offContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCascade_restrict(ctx *Cascade_restrictContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMaterialized_query_definition(ctx *Materialized_query_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRefreshable_table_options(ctx *Refreshable_table_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitColumn_alteration(ctx *Column_alterationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGeneration_alteration(ctx *Generation_alterationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIdentity_alteration(ctx *Identity_alterationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGeneration_attribute(ctx *Generation_attributeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAs_identity_clause(ctx *As_identity_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAs_identity_clause_opts(ctx *As_identity_clause_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPeriod_definition_alter(ctx *Period_definition_alterContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAdd_partition(ctx *Add_partitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBoundary_spec_alter(ctx *Boundary_spec_alterContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAttach_partition(ctx *Attach_partitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitActivate_deactivate(ctx *Activate_deactivateContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_tablespace_statement(ctx *Alter_tablespace_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_tablespace_opts(ctx *Alter_tablespace_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAdd_clause(ctx *Add_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_container_clause(ctx *Db_container_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_container_clause_opts(ctx *Db_container_clause_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDrop_container_clause(ctx *Drop_container_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFile_device(ctx *File_deviceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAll_containers_clause(ctx *All_containers_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSystem_container_clause(ctx *System_container_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitStripeset(ctx *StripesetContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitKm(ctx *KmContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitKmg_percent(ctx *Kmg_percentContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_threshold_statement(ctx *Alter_threshold_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_threshold_opts(ctx *Alter_threshold_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_threshold_predicate(ctx *Alter_threshold_predicateContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_threshold_exceeded_actions(ctx *Alter_threshold_exceeded_actionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDt_units(ctx *Dt_unitsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDt_units_with_seconds(ctx *Dt_units_with_secondsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_trigger_statement(ctx *Alter_trigger_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_trusted_context_statement(ctx *Alter_trusted_context_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_trusted_context_opts(ctx *Alter_trusted_context_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_trusted_context_opts_alter_opts(ctx *Alter_trusted_context_opts_alter_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAddr_clause_encryption_val(ctx *Addr_clause_encryption_valContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAddress_clause(ctx *Address_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUser_clause(ctx *User_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUse_for_opts(ctx *Use_for_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUse_for_opts_2(ctx *Use_for_opts_2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_type_statement(ctx *Alter_type_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_type_opts(ctx *Alter_type_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_identifier(ctx *Method_identifierContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_options(ctx *Method_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_usage_list_statement(ctx *Alter_usage_list_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_usage_list_opts_item(ctx *Alter_usage_list_opts_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_user_mapping_statement(ctx *Alter_user_mapping_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_user_mapping_opts_item(ctx *Alter_user_mapping_opts_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAdd_set(ctx *Add_setContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_view_statement(ctx *Alter_view_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_view_opts(ctx *Alter_view_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_work_action_set_statement(ctx *Alter_work_action_set_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_work_action_set_opts(ctx *Alter_work_action_set_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_action_alteration(ctx *Work_action_alterationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_action_alteration_opts(ctx *Work_action_alteration_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_action_types_clause(ctx *Alter_action_types_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitThreshold_predicate_clause(ctx *Threshold_predicate_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_work_class_set_statement(ctx *Alter_work_class_set_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_work_class_set_opts(ctx *Alter_work_class_set_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_class_alteration(ctx *Work_class_alterationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_class_alteration_opts(ctx *Work_class_alteration_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFor_from_to_alter_clause(ctx *For_from_to_alter_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSchema_alter_clause(ctx *Schema_alter_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_tag_alter_clause(ctx *Data_tag_alter_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_workload_statement(ctx *Alter_workload_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_workload_opts_item(ctx *Alter_workload_opts_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPackage_executable(ctx *Package_executableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBase_none(ctx *Base_noneContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExtended_base_none(ctx *Extended_base_noneContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_collect_activity_data_clause(ctx *Alter_collect_activity_data_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWith_opts(ctx *With_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_collect_history_clause(ctx *Alter_collect_history_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_collect_lock_wait_data_clause(ctx *Alter_collect_lock_wait_data_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_wrapper_statement(ctx *Alter_wrapper_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_wrapper_opts_item(ctx *Alter_wrapper_opts_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlter_xsrobject_statement(ctx *Alter_xsrobject_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitString(ctx *StringContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitString_constant(ctx *String_constantContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNumeric_constant(ctx *Numeric_constantContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_type(ctx *Data_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAnchored_data_type(ctx *Anchored_data_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAnchored_non_row_data_type(ctx *Anchored_non_row_data_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAnchored_row_data_type(ctx *Anchored_row_data_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSource_data_type(ctx *Source_data_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_type_constrainst(ctx *Data_type_constrainstContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCheck_condition(ctx *Check_conditionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_type_2(ctx *Data_type_2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBuilt_in_type(ctx *Built_in_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInteger_paren(ctx *Integer_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInteger_kmg_paren(ctx *Integer_kmg_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitChar_character(ctx *Char_characterContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOctets_codeunits(ctx *Octets_codeunitsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCodeunits(ctx *CodeunitsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitKmg(ctx *KmgContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRs_locator_variable(ctx *Rs_locator_variableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInteger_constant_list(ctx *Integer_constant_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInteger_constant(ctx *Integer_constantContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInteger_value(ctx *Integer_valueContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPositive_integer(ctx *Positive_integerContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBigint_value(ctx *Bigint_valueContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBigint_constant(ctx *Bigint_constantContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMember_number(ctx *Member_numberContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitVersion_id(ctx *Version_idContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDrop_statement(ctx *Drop_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlias_designator(ctx *Alias_designatorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitService_class_designator(ctx *Service_class_designatorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTablespace_name_list(ctx *Tablespace_name_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAssociate_locators_statement(ctx *Associate_locators_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAudit_statement(ctx *Audit_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBegin_declare_section_statement(ctx *Begin_declare_section_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCall_statement(ctx *Call_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitArg_list_paren(ctx *Arg_list_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitArg_list(ctx *Arg_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitArgument(ctx *ArgumentContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCase_statement(ctx *Case_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSearched_case_statement_when_clause(ctx *Searched_case_statement_when_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSimple_case_statement_when_clause(ctx *Simple_case_statement_when_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitClose_statement(ctx *Close_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitComment_statement(ctx *Comment_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitColumn_comment(ctx *Column_commentContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitComment_objects(ctx *Comment_objectsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCommit_statement(ctx *Commit_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitConnect_type_1_statement(ctx *Connect_type_1_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAuthorization(ctx *AuthorizationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPasswords(ctx *PasswordsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitLock_block(ctx *Lock_blockContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAccesstoken(ctx *AccesstokenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitToken(ctx *TokenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitApi_key(ctx *Api_keyContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitToken_type(ctx *Token_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDeclare_cursor_statement(ctx *Declare_cursor_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDeclare_global_temporary_table_statement(ctx *Declare_global_temporary_table_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDescribe_statement(ctx *Describe_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitXquery_statement(ctx *Xquery_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDescribe_input_statement(ctx *Describe_input_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDescribe_output_statement(ctx *Describe_output_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDisconnect_statement(ctx *Disconnect_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEnd_declare_section_statement(ctx *End_declare_section_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExecute_statement(ctx *Execute_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHost_variable_expression(ctx *Host_variable_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAssignment_target(ctx *Assignment_targetContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExecute_immediate_statement(ctx *Execute_immediate_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExplain_statement(ctx *Explain_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExplainable_sql_statement(ctx *Explainable_sql_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFetch_statement(ctx *Fetch_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFlush_bufferpools_statement(ctx *Flush_bufferpools_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFlush_event_monitor_statement(ctx *Flush_event_monitor_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFlush_federated_cache_statement(ctx *Flush_federated_cache_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFlush_optimization_profile_cache_statement(ctx *Flush_optimization_profile_cache_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFlush_package_cache_statement(ctx *Flush_package_cache_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFlush_authentication_cache_statement(ctx *Flush_authentication_cache_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFree_locator_statement(ctx *Free_locator_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGet_diagnostics_statement(ctx *Get_diagnostics_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitStatement_information(ctx *Statement_informationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCondition_information(ctx *Condition_informationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCondition_var_assignment(ctx *Condition_var_assignmentContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitLock_table_statement(ctx *Lock_table_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPipe_statement(ctx *Pipe_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRefresh_table_statement(ctx *Refresh_table_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRelease_connection_statement(ctx *Release_connection_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRename_statement(ctx *Rename_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRename_stogroup_statement(ctx *Rename_stogroup_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRename_tablespace_statement(ctx *Rename_tablespace_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSet_statement(ctx *Set_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAccess_mode_clause(ctx *Access_mode_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCascade_clause(ctx *Cascade_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTo_descendent_types(ctx *To_descendent_typesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_type_list(ctx *Table_type_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_type(ctx *Table_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_checked_options_list(ctx *Table_checked_options_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_checked_options(ctx *Table_checked_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOnline_options(ctx *Online_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitQuery_optimization_options(ctx *Query_optimization_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCheck_options(ctx *Check_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIncremental_options(ctx *Incremental_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitException_clause(ctx *Exception_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIn_table_use_clause(ctx *In_table_use_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_unchecked_options(ctx *Table_unchecked_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFull_access(ctx *Full_accessContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIntegrity_options(ctx *Integrity_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIntegrity_options_item(ctx *Integrity_options_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitVar_def_list(ctx *Var_def_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitVar_def(ctx *Var_defContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExpr_null(ctx *Expr_nullContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExpr_null_default(ctx *Expr_null_defaultContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitArray_index(ctx *Array_indexContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRow_fullselect(ctx *Row_fullselectContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTarget_variable(ctx *Target_variableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTarget_cursor_variable(ctx *Target_cursor_variableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTarget_row_variable(ctx *Target_row_variableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRow_array_element_specification(ctx *Row_array_element_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRow_field_reference(ctx *Row_field_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitField_reference(ctx *Field_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSearch_condition(ctx *Search_conditionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPredicate(ctx *PredicateContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAccording_to_clause(ctx *According_to_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitXml_schema_identification_list(ctx *Xml_schema_identification_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitXml_schema_identification(ctx *Xml_schema_identificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFullselect_in_parentheses(ctx *Fullselect_in_parenthesesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSome_any_all(ctx *Some_any_allContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRow_value_expression(ctx *Row_value_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitComparison_operator(ctx *Comparison_operatorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRow_expression(ctx *Row_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPath_opt_list(ctx *Path_opt_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPath_opt(ctx *Path_optContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPkg_opt_list(ctx *Pkg_opt_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPkg_opt(ctx *Pkg_optContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMaintain_opt_list(ctx *Maintain_opt_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMaintain_opt(ctx *Maintain_optContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitVariable(ctx *VariableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHost_variable(ctx *Host_variableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSet_integrity_statement(ctx *Set_integrity_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTransfer_ownership_statement(ctx *Transfer_ownership_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitObjects(ctx *ObjectsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWhenever_statement(ctx *Whenever_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFor_statement(ctx *For_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGoto_statement(ctx *Goto_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIf_statement(ctx *If_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInclude_statement(ctx *Include_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitResignal_statement(ctx *Resignal_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSignal_information(ctx *Signal_informationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDiagnostic_string_constant(ctx *Diagnostic_string_constantContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSignal_statement(ctx *Signal_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSqlstate_string_constant(ctx *Sqlstate_string_constantContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSqlstate_string_variable(ctx *Sqlstate_string_variableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSignal_information_2(ctx *Signal_information_2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDiagnostic_string_expression(ctx *Diagnostic_string_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIterate_statement(ctx *Iterate_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitLeave_statement(ctx *Leave_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitLoop_statement(ctx *Loop_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOpen_statement(ctx *Open_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitVariable_or_expression(ctx *Variable_or_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSelect_into_statement(ctx *Select_into_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitValues_into_statement(ctx *Values_into_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPrepare_statement(ctx *Prepare_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRepeat_statement(ctx *Repeat_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitReturn_statement(ctx *Return_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWhile_statement(ctx *While_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_routine_statement(ctx *Sql_routine_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCommon_table_expression(ctx *Common_table_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_alias_statement(ctx *Create_alias_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_alias(ctx *Table_aliasContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModule_alias(ctx *Module_aliasContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSequence_alias(ctx *Sequence_aliasContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOr_replace(ctx *Or_replaceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_audit_policy_statement(ctx *Create_audit_policy_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAudit_policy_opts(ctx *Audit_policy_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAudit_policy_categories_opts(ctx *Audit_policy_categories_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_bufferpool_statement(ctx *Create_bufferpool_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBufferpool_opts(ctx *Bufferpool_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExcept_clause(ctx *Except_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMember_list(ctx *Member_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMember_list_item(ctx *Member_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_database_partition_group_statement(ctx *Create_database_partition_group_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_event_monitor_statement(ctx *Create_event_monitor_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_event_monitor_activities_statement(ctx *Create_event_monitor_activities_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFormatted_event_table_info_3(ctx *Formatted_event_table_info_3Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_event_monitor_change_history_statement(ctx *Create_event_monitor_change_history_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEvent_control_list(ctx *Event_control_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEvent_control(ctx *Event_controlContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_event_monitor_locking_statement(ctx *Create_event_monitor_locking_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_event_monitor_package_cache_statement(ctx *Create_event_monitor_package_cache_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFilter_and_collection_options(ctx *Filter_and_collection_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEvent_condition(ctx *Event_conditionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEvent_condition_item(ctx *Event_condition_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_event_monitor_statistics_statement(ctx *Create_event_monitor_statistics_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEvent_monitor_statistics_opts(ctx *Event_monitor_statistics_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_event_monitor_threshold_violations_statement(ctx *Create_event_monitor_threshold_violations_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFormatted_event_table_info_2(ctx *Formatted_event_table_info_2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFile_options(ctx *File_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEvent_monitor_threshold_opts(ctx *Event_monitor_threshold_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPages(ctx *PagesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_event_monitor_unit_of_work(ctx *Create_event_monitor_unit_of_workContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFormatted_event_table_info(ctx *Formatted_event_table_infoContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAutostart_manualstart(ctx *Autostart_manualstartContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEvm_group(ctx *Evm_groupContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTarget_table_options(ctx *Target_table_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_external_table_statement(ctx *Create_external_table_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExt_table_option(ctx *Ext_table_optionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExt_table_option_value(ctx *Ext_table_option_valueContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_function_statement(ctx *Create_function_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_function_aggregate_interface_statement(ctx *Create_function_aggregate_interface_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAgg_fn_param_decl(ctx *Agg_fn_param_declContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAgg_fn_option_list(ctx *Agg_fn_option_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitState_variable_declaration(ctx *State_variable_declarationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_function_external_scalar_statement(ctx *Create_function_external_scalar_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExt_scalar_param_decl(ctx *Ext_scalar_param_declContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExt_scalar_option_list(ctx *Ext_scalar_option_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExt_scalar_option_list_item(ctx *Ext_scalar_option_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPredicate_specification(ctx *Predicate_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_filter(ctx *Data_filterContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIndex_exploitation(ctx *Index_exploitationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExploitation_rule(ctx *Exploitation_ruleContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_function_external_table_statement(ctx *Create_function_external_table_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExt_table_param_decl_list(ctx *Ext_table_param_decl_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExt_table_param_decl(ctx *Ext_table_param_declContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExt_table_option_list(ctx *Ext_table_option_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExt_table_option_list_item(ctx *Ext_table_option_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_function_old_db_external_function_statement(ctx *Create_function_old_db_external_function_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOledb_option_list(ctx *Oledb_option_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOledb_option_list_item(ctx *Oledb_option_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_function_sourced_or_template_statement(ctx *Create_function_sourced_or_template_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFn_return_opts(ctx *Fn_return_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFn_return_opts_item(ctx *Fn_return_opts_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTemplate_opts(ctx *Template_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTemplate_opts_item(ctx *Template_opts_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAscii_unicode(ctx *Ascii_unicodeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitParam_decl_list_3(ctx *Param_decl_list_3Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitParam_decl_3(ctx *Param_decl_3Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_function_sql_scalar_table_or_row_statement(ctx *Create_function_sql_scalar_table_or_row_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitParam_decl_list_2(ctx *Param_decl_list_2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitParam_decl_2(ctx *Param_decl_2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_function_body(ctx *Sql_function_bodyContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_function_mapping_statement(ctx *Create_function_mapping_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFunction_options(ctx *Function_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFunction_option_name(ctx *Function_option_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_global_temporary_table_statement(ctx *Create_global_temporary_table_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_global_temporary_table_opts(ctx *Create_global_temporary_table_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_global_temporary_table_item(ctx *Create_global_temporary_table_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDelete_preserve(ctx *Delete_preserveContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_histogram_template_statement(ctx *Create_histogram_template_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_index_statement(ctx *Create_index_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIndex_col_opts(ctx *Index_col_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIndex_col_opts_item(ctx *Index_col_opts_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitKey_expression(ctx *Key_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_index_extension_statement(ctx *Create_index_extension_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitParam_list(ctx *Param_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIndex_maintenance(ctx *Index_maintenanceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_function_invocation(ctx *Table_function_invocationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIndex_search(ctx *Index_searchContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSearch_method_definition(ctx *Search_method_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_mask_statement(ctx *Create_mask_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCase_expression(ctx *Case_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRange_producing_funciton_invocation(ctx *Range_producing_funciton_invocationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIndex_filtering_function_invocation(ctx *Index_filtering_function_invocationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_method_statement(ctx *Create_method_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_opts(ctx *Method_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_opts_item(ctx *Method_opts_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_signature(ctx *Method_signatureContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_param_list(ctx *Method_param_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_type_3(ctx *Data_type_3Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_type_4(ctx *Data_type_4Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_method_body(ctx *Sql_method_bodyContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCompound_sql_inlined(ctx *Compound_sql_inlinedContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_statement_inlined(ctx *Sql_statement_inlinedContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCompound_sql_compiled(ctx *Compound_sql_compiledContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_statement_compiled(ctx *Sql_statement_compiledContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_module_statement(ctx *Create_module_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_nickname_statement(ctx *Create_nickname_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNick_name_option_name(ctx *Nick_name_option_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRemote_object_name(ctx *Remote_object_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNon_relational_data_definition(ctx *Non_relational_data_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNick_name_column_list(ctx *Nick_name_column_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNick_name_column_list_item(ctx *Nick_name_column_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNick_name_column_definition(ctx *Nick_name_column_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNick_name_column_options(ctx *Nick_name_column_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFederated_column_options(ctx *Federated_column_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitColumn_option_name(ctx *Column_option_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_permission_statement(ctx *Create_permission_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_procedure_statement(ctx *Create_procedure_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_procedure_external_statement(ctx *Create_procedure_external_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitProc_ext_param_list(ctx *Proc_ext_param_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitProc_ext_param(ctx *Proc_ext_paramContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOption_list_2(ctx *Option_list_2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOption_list_2_item(ctx *Option_list_2_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_procedure_sourced_statement(ctx *Create_procedure_sourced_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSource_procedure_clause(ctx *Source_procedure_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSource_object_name(ctx *Source_object_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOption_list_1(ctx *Option_list_1Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOption_list_1_item(ctx *Option_list_1_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitResult_set_element_number(ctx *Result_set_element_numberContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUnique_id(ctx *Unique_idContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_procedure_sql_statement(ctx *Create_procedure_sql_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitProc_parameter_list(ctx *Proc_parameter_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitProc_parameter_list_item(ctx *Proc_parameter_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIn_out_inout(ctx *In_out_inoutContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOption_list(ctx *Option_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOption_list_item(ctx *Option_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_procedure_body(ctx *Sql_procedure_bodyContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_role_statement(ctx *Create_role_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_schema_statement(ctx *Create_schema_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSchema_sql_statement(ctx *Schema_sql_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_security_label_component_statement(ctx *Create_security_label_component_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitArray_clause(ctx *Array_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSet_clause(ctx *Set_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTree_clause(ctx *Tree_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTree_clause_item(ctx *Tree_clause_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_security_label_statement(ctx *Create_security_label_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_security_label_item(ctx *Create_security_label_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_security_policy_statement(ctx *Create_security_policy_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_sequence_statement(ctx *Create_sequence_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_sequence_opts(ctx *Create_sequence_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_sequence_opts_item(ctx *Create_sequence_opts_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_service_class_statement(ctx *Create_service_class_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHigh_medium_low(ctx *High_medium_lowContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOn_off(ctx *On_offContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSoft_hard(ctx *Soft_hardContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_server_statement(ctx *Create_server_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPassword_(ctx *Password_Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_stogroup_statement(ctx *Create_stogroup_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_stogroup_opts(ctx *Create_stogroup_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_synonym_statement(ctx *Create_synonym_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_table_statement(ctx *Create_table_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_table_opts(ctx *Create_table_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_option_list(ctx *Table_option_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_option_list_item(ctx *Table_option_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_option_name(ctx *Table_option_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitElement_list(ctx *Element_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitElement_list_item(ctx *Element_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitColumn_definition(ctx *Column_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPeriod_definition(ctx *Period_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUnique_constraint(ctx *Unique_constraintContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitReferential_constraint(ctx *Referential_constraintContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCheck_constraint(ctx *Check_constraintContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitColumn_options(ctx *Column_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitColumn_options_item(ctx *Column_options_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitReferences_clause(ctx *References_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRule_clause(ctx *Rule_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitConstraint_attributes(ctx *Constraint_attributesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDefault_clause(ctx *Default_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDefault_values(ctx *Default_valuesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGenerated_clause(ctx *Generated_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDatetime_special_register(ctx *Datetime_special_registerContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUser_special_register(ctx *User_special_registerContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCast_function(ctx *Cast_functionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIdentity_options(ctx *Identity_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIdentity_options_item(ctx *Identity_options_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAs_row_change_timestamp_clause(ctx *As_row_change_timestamp_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAs_generated_expression_clause(ctx *As_generated_expression_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGeneration_expression(ctx *Generation_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAs_row_transaction_timestamp_clause(ctx *As_row_transaction_timestamp_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAs_row_transaction_start_id_clause(ctx *As_row_transaction_start_id_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOid_column_definition(ctx *Oid_column_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRange_partition_spec(ctx *Range_partition_specContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPartition_expression_list(ctx *Partition_expression_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPartition_expression(ctx *Partition_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPartition_element_list(ctx *Partition_element_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPartition_element(ctx *Partition_elementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBoundary_spec(ctx *Boundary_specContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPartition_tablespace_options(ctx *Partition_tablespace_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDuration_label(ctx *Duration_labelContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitStarting_clause(ctx *Starting_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitConst_min_max_list(ctx *Const_min_max_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitConst_min_max(ctx *Const_min_maxContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEnding_clause(ctx *Ending_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTyped_table_options(ctx *Typed_table_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTyped_element_list(ctx *Typed_element_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTyped_element_list_item(ctx *Typed_element_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAs_result_table(ctx *As_result_tableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCopy_options(ctx *Copy_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMaterialized_query_options(ctx *Materialized_query_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitStaging_table_definition(ctx *Staging_table_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDimensions_clause(ctx *Dimensions_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCol_names(ctx *Col_namesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSequence_key_spec(ctx *Sequence_key_specContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSequence_key_spec_list(ctx *Sequence_key_spec_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSequence_key_spec_list_item(ctx *Sequence_key_spec_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTablespace_clauses(ctx *Tablespace_clausesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDistribution_clause(ctx *Distribution_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPartitioning_clause(ctx *Partitioning_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIf_not_exists(ctx *If_not_existsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_tablespace_statement(ctx *Create_tablespace_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitStorage_group(ctx *Storage_groupContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSize_attributes(ctx *Size_attributesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSystem_containers(ctx *System_containersContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitContainer_string_list(ctx *Container_string_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDatabase_containers(ctx *Database_containersContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitContainer_clause(ctx *Container_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitContainer_clause_list(ctx *Container_clause_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitContainer_clause_list_item(ctx *Container_clause_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOn_db_partitions_clause(ctx *On_db_partitions_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_partition_number_list(ctx *Db_partition_number_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_partition_number_list_item(ctx *Db_partition_number_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_partition_number(ctx *Db_partition_numberContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNumber_of_pages(ctx *Number_of_pagesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNumber_of_files(ctx *Number_of_filesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNumber_of_milliseconds(ctx *Number_of_millisecondsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNumber_megabytes_per_second(ctx *Number_megabytes_per_secondContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_threshold_statement(ctx *Create_threshold_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitThreshold_domain(ctx *Threshold_domainContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitStatement_text(ctx *Statement_textContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExecutable_id(ctx *Executable_idContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEnforcement_scope(ctx *Enforcement_scopeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitThreshold_predicate(ctx *Threshold_predicateContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitChecking_every(ctx *Checking_everyContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHour_to_seconds(ctx *Hour_to_secondsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDay_to_minutes(ctx *Day_to_minutesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDay_to_seconds(ctx *Day_to_secondsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitThreshold_exceeded_actions_2(ctx *Threshold_exceeded_actions_2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDetails_section(ctx *Details_sectionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRemap_activity_action(ctx *Remap_activity_actionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_transform_statement(ctx *Create_transform_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTranform_list(ctx *Tranform_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTranform_list_item(ctx *Tranform_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTransform_group_list(ctx *Transform_group_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTransform_group_list_item(ctx *Transform_group_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_trigger_statement(ctx *Create_trigger_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRef_list(ctx *Ref_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRef_list_item(ctx *Ref_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOld_new(ctx *Old_newContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCorrelation_name(ctx *Correlation_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIdentifier(ctx *IdentifierContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTrigger_event(ctx *Trigger_eventContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTriggered_action(ctx *Triggered_actionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_procedure_statement(ctx *Sql_procedure_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_function_statement(ctx *Sql_function_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_trusted_context_statement(ctx *Create_trusted_context_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAttr_list(ctx *Attr_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAttr_list_item(ctx *Attr_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAuth_list(ctx *Auth_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAuth_list_item(ctx *Auth_list_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAddress_value(ctx *Address_valueContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEncryption_value(ctx *Encryption_valueContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_type_statement(ctx *Create_type_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_type_array_statement(ctx *Create_type_array_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_type_cursor_statement(ctx *Create_type_cursor_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_type_distinct_statement(ctx *Create_type_distinct_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_type_row_statement(ctx *Create_type_row_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitField_definition_list_paren(ctx *Field_definition_list_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitField_definition_list(ctx *Field_definition_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitField_definition(ctx *Field_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_type_structured_statement(ctx *Create_type_structured_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitStructured_type_seq(ctx *Structured_type_seqContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAttribute_definition_list_paren(ctx *Attribute_definition_list_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAttribute_definition_list(ctx *Attribute_definition_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAttribute_definition(ctx *Attribute_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_specification_list(ctx *Method_specification_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_specification(ctx *Method_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_specification_seq(ctx *Method_specification_seqContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAs_locator(ctx *As_locatorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitParam_decl_list_paren(ctx *Param_decl_list_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitParam_decl_list(ctx *Param_decl_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitParam_decl(ctx *Param_declContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_routine_characteristics(ctx *Sql_routine_characteristicsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExternal_routine_characteristics(ctx *External_routine_characteristicsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitLength(ctx *LengthContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRep_type(ctx *Rep_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitVarchars(ctx *VarcharsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitVarbinaries(ctx *VarbinariesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFor_bit_data(ctx *For_bit_dataContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitLob_options(ctx *Lob_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_type_mapping_statement(ctx *Create_type_mapping_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFor_bit_data_precision(ctx *For_bit_data_precisionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPrecision(ctx *PrecisionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitScale(ctx *ScaleContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPrecision_scale_comp(ctx *Precision_scale_compContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFrom_to(ctx *From_toContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_source_data_type(ctx *Data_source_data_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitLocal_data_type(ctx *Local_data_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRemote_server(ctx *Remote_serverContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitServer_version(ctx *Server_versionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitServer_type(ctx *Server_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitVersion(ctx *VersionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRelease(ctx *ReleaseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMod(ctx *ModContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_usage_list_statement(ctx *Create_usage_list_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_user_mapping_statement(ctx *Create_user_mapping_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUser_mapping_options_paren(ctx *User_mapping_options_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUser_mapping_options(ctx *User_mapping_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_variable_statement(ctx *Create_variable_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitConstant_(ctx *Constant_Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSpecial_register(ctx *Special_registerContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGlobal_variable(ctx *Global_variableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_type_1(ctx *Data_type_1Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCursor_value_constructor(ctx *Cursor_value_constructorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAnchored_variable_data_type(ctx *Anchored_variable_data_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHoldability(ctx *HoldabilityContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitReturnability(ctx *ReturnabilityContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_view_statement(ctx *Create_view_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_view_seq(ctx *Create_view_seqContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFullselect(ctx *FullselectContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSubselect(ctx *SubselectContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSelect_clause(ctx *Select_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSelect_clause_item(ctx *Select_clause_itemContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFrom_clause(ctx *From_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_reference(ctx *Table_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_reference_list(ctx *Table_reference_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSingles_table_reference(ctx *Singles_table_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPeriod_specification(ctx *Period_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitValue(ctx *ValueContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCorrelation_clause(ctx *Correlation_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTablesample_clause(ctx *Tablesample_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNumeric_expression(ctx *Numeric_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSingle_view_reference(ctx *Single_view_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSingle_nickname_reference(ctx *Single_nickname_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOnly_table_reference(ctx *Only_table_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOuter_table_reference(ctx *Outer_table_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAnalyze_table_reference(ctx *Analyze_table_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitImplementation_clause(ctx *Implementation_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNested_table_reference(ctx *Nested_table_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitContinue_handler(ctx *Continue_handlerContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSpecific_condition_value(ctx *Specific_condition_valueContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_change_table_reference(ctx *Data_change_table_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSearched_update_statement(ctx *Searched_update_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSearched_delete_statement(ctx *Searched_delete_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFinal_new(ctx *Final_newContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFinal_new_old(ctx *Final_new_oldContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_function_reference(ctx *Table_function_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_udf_cardinality_clause(ctx *Table_udf_cardinality_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTyped_correlation_clause(ctx *Typed_correlation_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitColumn_name_data_type(ctx *Column_name_data_typeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCollection_derived_table(ctx *Collection_derived_tableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_function(ctx *Table_functionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitXmltable_expression(ctx *Xmltable_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitXmltable_function(ctx *Xmltable_functionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitJoined_table(ctx *Joined_tableContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitJoin_condition(ctx *Join_conditionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOuter(ctx *OuterContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExternal_table_reference(ctx *External_table_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitColumn_definition_2(ctx *Column_definition_2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFile_name(ctx *File_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWhere_clause(ctx *Where_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGroup_by_clause(ctx *Group_by_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGroup_by_clause_opts(ctx *Group_by_clause_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrouping_expression(ctx *Grouping_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrouping_sets(ctx *Grouping_setsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSuper_groups(ctx *Super_groupsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGrant_total(ctx *Grant_totalContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHaving_clause(ctx *Having_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOrder_by_clause(ctx *Order_by_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOrder_by_clause_opts(ctx *Order_by_clause_optsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_designator(ctx *Table_designatorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAsc_desc(ctx *Asc_descContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFirst_last(ctx *First_lastContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSort_key(ctx *Sort_keyContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSimple_column_name(ctx *Simple_column_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSimple_integer(ctx *Simple_integerContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSork_key_expression(ctx *Sork_key_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOffset_clause(ctx *Offset_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOffset_row_count(ctx *Offset_row_countContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFetch_clause(ctx *Fetch_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFetch_row_count(ctx *Fetch_row_countContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRow_rows(ctx *Row_rowsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIsolation_clause(ctx *Isolation_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitLock_request_clause(ctx *Lock_request_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitValues_clause(ctx *Values_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitValues_row(ctx *Values_rowContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRoot_view_definition(ctx *Root_view_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSubview_definition(ctx *Subview_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOid_column(ctx *Oid_columnContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWith_options(ctx *With_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWith_option_def(ctx *With_option_defContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWith_option_scope_def(ctx *With_option_scope_defContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUnder_clause(ctx *Under_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_work_action_set_statement(ctx *Create_work_action_set_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_action_definition_list_paren(ctx *Work_action_definition_list_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_action_definition_list(ctx *Work_action_definition_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_action_definition(ctx *Work_action_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAction_types_clause(ctx *Action_types_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitThreshold_types_clause(ctx *Threshold_types_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSecond_seconds(ctx *Second_secondsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHours_minutes(ctx *Hours_minutesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitThreshold_exceeded_actions(ctx *Threshold_exceeded_actionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCollect_activity_data_clause(ctx *Collect_activity_data_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWith_without(ctx *With_withoutContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHistogram_templace_clause(ctx *Histogram_templace_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_work_class_set_statement(ctx *Create_work_class_set_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_class_definition_list_paren(ctx *Work_class_definition_list_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_class_definition_list(ctx *Work_class_definition_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_class_definition(ctx *Work_class_definitionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_attributes(ctx *Work_attributesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPosition_clause(ctx *Position_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPosition_(ctx *Position_Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFor_from_to_clause(ctx *For_from_to_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFrom_value(ctx *From_valueContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTo_value(ctx *To_valueContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_tag_clause(ctx *Data_tag_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSchema_clause(ctx *Schema_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_workload_statement(ctx *Create_workload_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPkg_exec_seq(ctx *Pkg_exec_seqContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPosition_clause_2(ctx *Position_clause_2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitConnection_attributes(ctx *Connection_attributesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitString_list(ctx *String_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitString_list_paren(ctx *String_list_parenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWorkload_attributes(ctx *Workload_attributesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDegree(ctx *DegreeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAllow_disallow(ctx *Allow_disallowContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCollect_on_clause(ctx *Collect_on_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCollect_details_clause(ctx *Collect_details_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCollect_lock_wait_options(ctx *Collect_lock_wait_optionsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWait_time(ctx *Wait_timeContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCreate_wrapper_statement(ctx *Create_wrapper_statementContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWrapper_option_list(ctx *Wrapper_option_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWrapper_option(ctx *Wrapper_optionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExpression(ctx *ExpressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFunction_invocation(ctx *Function_invocationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAll_distinct(ctx *All_distinctContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitScalar_fullselect(ctx *Scalar_fullselectContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCast_specification(ctx *Cast_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCursor_cast_specification(ctx *Cursor_cast_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRow_cast_specification(ctx *Row_cast_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInterval_cast_specification(ctx *Interval_cast_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitXmlcast_specification(ctx *Xmlcast_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitArray_element_specification(ctx *Array_element_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitArray_constructor(ctx *Array_constructorContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_invocation(ctx *Method_invocationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOlap_specification(ctx *Olap_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOrdered_olap_specification(ctx *Ordered_olap_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWindow_partition_clause(ctx *Window_partition_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWindow_order_clause(ctx *Window_order_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNumbering_specification(ctx *Numbering_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAggregation_specification(ctx *Aggregation_specificationContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOlap_aggregate_function(ctx *Olap_aggregate_functionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFirst_value_function(ctx *First_value_functionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitLast_value_function(ctx *Last_value_functionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNth_value_function(ctx *Nth_value_functionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRatio_to_report_function(ctx *Ratio_to_report_functionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIgnore_respect_nulls(ctx *Ignore_respect_nullsContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFrom_first_last(ctx *From_first_lastContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWindow_aggregation_group_clause(ctx *Window_aggregation_group_clauseContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGroup_start(ctx *Group_startContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGroup_between(ctx *Group_betweenContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGroup_bound1(ctx *Group_bound1Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGroup_bound2(ctx *Group_bound2Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGroup_end(ctx *Group_endContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRow_change_expression(ctx *Row_change_expressionContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSequence_reference(ctx *Sequence_referenceContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSubtype_treatment(ctx *Subtype_treatmentContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExpression_list(ctx *Expression_listContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExpression_list_in_parentheses(ctx *Expression_list_in_parenthesesContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitId_(ctx *Id_Context) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExposed_name(ctx *Exposed_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitName(ctx *NameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitLabel(ctx *LabelContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHost_label(ctx *Host_labelContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitLibrary_name(ctx *Library_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitArray_type_name(ctx *Array_type_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAttribute_name(ctx *Attribute_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRow_type_name(ctx *Row_type_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAuthorization_name(ctx *Authorization_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBoolean_variable_name(ctx *Boolean_variable_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitArray_variable_name(ctx *Array_variable_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitColumn_name(ctx *Column_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitConstraint_name(ctx *Constraint_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDescriptor_name(ctx *Descriptor_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDistinct_type_name(ctx *Distinct_type_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCursor_name(ctx *Cursor_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCursor_type_name(ctx *Cursor_type_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCondition_name(ctx *Condition_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitData_source_name(ctx *Data_source_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitExpression_name(ctx *Expression_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGroup_name(ctx *Group_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPolicy_name(ctx *Policy_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitBufferpool_name(ctx *Bufferpool_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_partition_name(ctx *Db_partition_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDatabase_name(ctx *Database_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitEvent_monitor_name(ctx *Event_monitor_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitField_name(ctx *Field_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFor_loop_name(ctx *For_loop_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFunction_name(ctx *Function_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitFunction_mapping_name(ctx *Function_mapping_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitGlobal_variable_name(ctx *Global_variable_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHierarchy_name(ctx *Hierarchy_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHost_variable_name(ctx *Host_variable_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitParameter_marker(ctx *Parameter_markerContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTemplate_name(ctx *Template_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIndex_name(ctx *Index_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitIndex_extension_name(ctx *Index_extension_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitInput_descriptor_name(ctx *Input_descriptor_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMask_name(ctx *Mask_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitMethod_name(ctx *Method_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModel_name(ctx *Model_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitModule_name(ctx *Module_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNew_owner(ctx *New_ownerContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitNick_name(ctx *Nick_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitObject_name(ctx *Object_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOid_column_name(ctx *Oid_column_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitOptimization_profile_name(ctx *Optimization_profile_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPackage_name(ctx *Package_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPartition_name(ctx *Partition_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPath_name(ctx *Path_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPermission_name(ctx *Permission_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPipe_name(ctx *Pipe_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitProcedure_name(ctx *Procedure_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitResult_descriptor_name(ctx *Result_descriptor_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRole_name(ctx *Role_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRoot_table_name(ctx *Root_table_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRoot_view_name(ctx *Root_view_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitRow_variable_name(ctx *Row_variable_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSource_schema_name(ctx *Source_schema_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSource_package_name(ctx *Source_package_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSource_procedure_name(ctx *Source_procedure_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_parameter_name(ctx *Sql_parameter_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSql_variable_name(ctx *Sql_variable_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTransition_variable_name(ctx *Transition_variable_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSavepoint_name(ctx *Savepoint_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSpecific_name(ctx *Specific_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSchema(ctx *SchemaContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSchema_name(ctx *Schema_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSearch_method_name(ctx *Search_method_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitServer_name(ctx *Server_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitServer_option_name(ctx *Server_option_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSession_authorization_name(ctx *Session_authorization_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitComponent_name(ctx *Component_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSec_label_comp_name(ctx *Sec_label_comp_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSecurity_policy_name(ctx *Security_policy_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSecurity_label_name(ctx *Security_label_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSequence_name(ctx *Sequence_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitService_class_name(ctx *Service_class_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitService_superclass_name(ctx *Service_superclass_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitStoragegroup_name(ctx *Storagegroup_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSupertype_name(ctx *Supertype_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSuperview_name(ctx *Superview_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitService_subclass_name(ctx *Service_subclass_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitStatement_name(ctx *Statement_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTable_name(ctx *Table_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTablespace_name(ctx *Tablespace_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTarget_identifier(ctx *Target_identifierContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitThreshold_name(ctx *Threshold_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTrigger_name(ctx *Trigger_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitContext_name(ctx *Context_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUsage_list_name(ctx *Usage_list_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitType_name(ctx *Type_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitType_mapping_name(ctx *Type_mapping_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTyped_table_name(ctx *Typed_table_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTyped_view_name(ctx *Typed_view_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUser_mapping_option_name(ctx *User_mapping_option_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitView_name(ctx *View_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitVariable_name(ctx *Variable_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_action_set_name(ctx *Work_action_set_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_class_set_name(ctx *Work_class_set_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWorkload_name(ctx *Workload_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_action_name(ctx *Work_action_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWork_class_name(ctx *Work_class_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWrapper_name(ctx *Wrapper_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitWrapper_option_name(ctx *Wrapper_option_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitXsrobject_name(ctx *Xsrobject_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitParameter_name(ctx *Parameter_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitCursor_variable_name(ctx *Cursor_variable_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitAlias_name(ctx *Alias_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitDb_partition_group_name(ctx *Db_partition_group_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSource_index_name(ctx *Source_index_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSource_table_name(ctx *Source_table_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSource_storagegroup_name(ctx *Source_storagegroup_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTarget_storagegroup_name(ctx *Target_storagegroup_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitSource_tablespace_name(ctx *Source_tablespace_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTarget_tablespace_name(ctx *Target_tablespace_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUnqualified_function_name(ctx *Unqualified_function_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUnqualified_procedure_name(ctx *Unqualified_procedure_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitUnqualified_specific_name(ctx *Unqualified_specific_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitPeriod_name(ctx *Period_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitHistory_table_name(ctx *History_table_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitXml_schema_name(ctx *Xml_schema_nameContext) interface{} {
	return v.VisitChildren(ctx)
}

func (v *BaseDb2ParserVisitor) VisitTodo(ctx *TodoContext) interface{} {
	return v.VisitChildren(ctx)
}
