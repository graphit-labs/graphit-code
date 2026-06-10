// Code generated from Db2Parser.g4 by ANTLR 4.13.2. DO NOT EDIT.

package db2 // Db2Parser
import "github.com/antlr4-go/antlr/v4"

// BaseDb2ParserListener is a complete listener for a parse tree produced by Db2Parser.
type BaseDb2ParserListener struct{}

var _ Db2ParserListener = &BaseDb2ParserListener{}

// VisitTerminal is called when a terminal node is visited.
func (s *BaseDb2ParserListener) VisitTerminal(node antlr.TerminalNode) {}

// VisitErrorNode is called when an error node is visited.
func (s *BaseDb2ParserListener) VisitErrorNode(node antlr.ErrorNode) {}

// EnterEveryRule is called when any rule is entered.
func (s *BaseDb2ParserListener) EnterEveryRule(ctx antlr.ParserRuleContext) {}

// ExitEveryRule is called when any rule is exited.
func (s *BaseDb2ParserListener) ExitEveryRule(ctx antlr.ParserRuleContext) {}

// EnterDb2_file is called when production db2_file is entered.
func (s *BaseDb2ParserListener) EnterDb2_file(ctx *Db2_fileContext) {}

// ExitDb2_file is called when production db2_file is exited.
func (s *BaseDb2ParserListener) ExitDb2_file(ctx *Db2_fileContext) {}

// EnterBatch is called when production batch is entered.
func (s *BaseDb2ParserListener) EnterBatch(ctx *BatchContext) {}

// ExitBatch is called when production batch is exited.
func (s *BaseDb2ParserListener) ExitBatch(ctx *BatchContext) {}

// EnterSql_statement is called when production sql_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_statement(ctx *Sql_statementContext) {}

// ExitSql_statement is called when production sql_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_statement(ctx *Sql_statementContext) {}

// EnterSql_schema_statement is called when production sql_schema_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_schema_statement(ctx *Sql_schema_statementContext) {}

// ExitSql_schema_statement is called when production sql_schema_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_schema_statement(ctx *Sql_schema_statementContext) {}

// EnterSql_data_change_statement is called when production sql_data_change_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_data_change_statement(ctx *Sql_data_change_statementContext) {
}

// ExitSql_data_change_statement is called when production sql_data_change_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_data_change_statement(ctx *Sql_data_change_statementContext) {
}

// EnterSql_data_statement is called when production sql_data_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_data_statement(ctx *Sql_data_statementContext) {}

// ExitSql_data_statement is called when production sql_data_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_data_statement(ctx *Sql_data_statementContext) {}

// EnterSql_transaction_statement is called when production sql_transaction_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_transaction_statement(ctx *Sql_transaction_statementContext) {
}

// ExitSql_transaction_statement is called when production sql_transaction_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_transaction_statement(ctx *Sql_transaction_statementContext) {
}

// EnterSql_connection_statement is called when production sql_connection_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_connection_statement(ctx *Sql_connection_statementContext) {}

// ExitSql_connection_statement is called when production sql_connection_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_connection_statement(ctx *Sql_connection_statementContext) {}

// EnterSql_dynamic_statement is called when production sql_dynamic_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_dynamic_statement(ctx *Sql_dynamic_statementContext) {}

// ExitSql_dynamic_statement is called when production sql_dynamic_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_dynamic_statement(ctx *Sql_dynamic_statementContext) {}

// EnterSql_session_statement is called when production sql_session_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_session_statement(ctx *Sql_session_statementContext) {}

// ExitSql_session_statement is called when production sql_session_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_session_statement(ctx *Sql_session_statementContext) {}

// EnterSql_embedded_host_language_statement is called when production sql_embedded_host_language_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_embedded_host_language_statement(ctx *Sql_embedded_host_language_statementContext) {
}

// ExitSql_embedded_host_language_statement is called when production sql_embedded_host_language_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_embedded_host_language_statement(ctx *Sql_embedded_host_language_statementContext) {
}

// EnterSql_constrol_statement is called when production sql_constrol_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_constrol_statement(ctx *Sql_constrol_statementContext) {}

// ExitSql_constrol_statement is called when production sql_constrol_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_constrol_statement(ctx *Sql_constrol_statementContext) {}

// EnterSelect_statement is called when production select_statement is entered.
func (s *BaseDb2ParserListener) EnterSelect_statement(ctx *Select_statementContext) {}

// ExitSelect_statement is called when production select_statement is exited.
func (s *BaseDb2ParserListener) ExitSelect_statement(ctx *Select_statementContext) {}

// EnterRead_only_clause is called when production read_only_clause is entered.
func (s *BaseDb2ParserListener) EnterRead_only_clause(ctx *Read_only_clauseContext) {}

// ExitRead_only_clause is called when production read_only_clause is exited.
func (s *BaseDb2ParserListener) ExitRead_only_clause(ctx *Read_only_clauseContext) {}

// EnterUpdate_clause is called when production update_clause is entered.
func (s *BaseDb2ParserListener) EnterUpdate_clause(ctx *Update_clauseContext) {}

// ExitUpdate_clause is called when production update_clause is exited.
func (s *BaseDb2ParserListener) ExitUpdate_clause(ctx *Update_clauseContext) {}

// EnterOptimize_for_clause is called when production optimize_for_clause is entered.
func (s *BaseDb2ParserListener) EnterOptimize_for_clause(ctx *Optimize_for_clauseContext) {}

// ExitOptimize_for_clause is called when production optimize_for_clause is exited.
func (s *BaseDb2ParserListener) ExitOptimize_for_clause(ctx *Optimize_for_clauseContext) {}

// EnterConcurrent_access_resolution_clause is called when production concurrent_access_resolution_clause is entered.
func (s *BaseDb2ParserListener) EnterConcurrent_access_resolution_clause(ctx *Concurrent_access_resolution_clauseContext) {
}

// ExitConcurrent_access_resolution_clause is called when production concurrent_access_resolution_clause is exited.
func (s *BaseDb2ParserListener) ExitConcurrent_access_resolution_clause(ctx *Concurrent_access_resolution_clauseContext) {
}

// EnterDelete_statement is called when production delete_statement is entered.
func (s *BaseDb2ParserListener) EnterDelete_statement(ctx *Delete_statementContext) {}

// ExitDelete_statement is called when production delete_statement is exited.
func (s *BaseDb2ParserListener) ExitDelete_statement(ctx *Delete_statementContext) {}

// EnterDelete_statement_searched_delete is called when production delete_statement_searched_delete is entered.
func (s *BaseDb2ParserListener) EnterDelete_statement_searched_delete(ctx *Delete_statement_searched_deleteContext) {
}

// ExitDelete_statement_searched_delete is called when production delete_statement_searched_delete is exited.
func (s *BaseDb2ParserListener) ExitDelete_statement_searched_delete(ctx *Delete_statement_searched_deleteContext) {
}

// EnterTable_or_view_name is called when production table_or_view_name is entered.
func (s *BaseDb2ParserListener) EnterTable_or_view_name(ctx *Table_or_view_nameContext) {}

// ExitTable_or_view_name is called when production table_or_view_name is exited.
func (s *BaseDb2ParserListener) ExitTable_or_view_name(ctx *Table_or_view_nameContext) {}

// EnterDelete_statement_positioned_delete is called when production delete_statement_positioned_delete is entered.
func (s *BaseDb2ParserListener) EnterDelete_statement_positioned_delete(ctx *Delete_statement_positioned_deleteContext) {
}

// ExitDelete_statement_positioned_delete is called when production delete_statement_positioned_delete is exited.
func (s *BaseDb2ParserListener) ExitDelete_statement_positioned_delete(ctx *Delete_statement_positioned_deleteContext) {
}

// EnterDelete_deltalake_statement is called when production delete_deltalake_statement is entered.
func (s *BaseDb2ParserListener) EnterDelete_deltalake_statement(ctx *Delete_deltalake_statementContext) {
}

// ExitDelete_deltalake_statement is called when production delete_deltalake_statement is exited.
func (s *BaseDb2ParserListener) ExitDelete_deltalake_statement(ctx *Delete_deltalake_statementContext) {
}

// EnterInsert_statement is called when production insert_statement is entered.
func (s *BaseDb2ParserListener) EnterInsert_statement(ctx *Insert_statementContext) {}

// ExitInsert_statement is called when production insert_statement is exited.
func (s *BaseDb2ParserListener) ExitInsert_statement(ctx *Insert_statementContext) {}

// EnterInsert_datalake_statement is called when production insert_datalake_statement is entered.
func (s *BaseDb2ParserListener) EnterInsert_datalake_statement(ctx *Insert_datalake_statementContext) {
}

// ExitInsert_datalake_statement is called when production insert_datalake_statement is exited.
func (s *BaseDb2ParserListener) ExitInsert_datalake_statement(ctx *Insert_datalake_statementContext) {
}

// EnterValues_item is called when production values_item is entered.
func (s *BaseDb2ParserListener) EnterValues_item(ctx *Values_itemContext) {}

// ExitValues_item is called when production values_item is exited.
func (s *BaseDb2ParserListener) ExitValues_item(ctx *Values_itemContext) {}

// EnterMerge_statement is called when production merge_statement is entered.
func (s *BaseDb2ParserListener) EnterMerge_statement(ctx *Merge_statementContext) {}

// ExitMerge_statement is called when production merge_statement is exited.
func (s *BaseDb2ParserListener) ExitMerge_statement(ctx *Merge_statementContext) {}

// EnterTable_view_fullselect is called when production table_view_fullselect is entered.
func (s *BaseDb2ParserListener) EnterTable_view_fullselect(ctx *Table_view_fullselectContext) {}

// ExitTable_view_fullselect is called when production table_view_fullselect is exited.
func (s *BaseDb2ParserListener) ExitTable_view_fullselect(ctx *Table_view_fullselectContext) {}

// EnterCommon_table_expression_list is called when production common_table_expression_list is entered.
func (s *BaseDb2ParserListener) EnterCommon_table_expression_list(ctx *Common_table_expression_listContext) {
}

// ExitCommon_table_expression_list is called when production common_table_expression_list is exited.
func (s *BaseDb2ParserListener) ExitCommon_table_expression_list(ctx *Common_table_expression_listContext) {
}

// EnterMatching_condition is called when production matching_condition is entered.
func (s *BaseDb2ParserListener) EnterMatching_condition(ctx *Matching_conditionContext) {}

// ExitMatching_condition is called when production matching_condition is exited.
func (s *BaseDb2ParserListener) ExitMatching_condition(ctx *Matching_conditionContext) {}

// EnterModification_operation is called when production modification_operation is entered.
func (s *BaseDb2ParserListener) EnterModification_operation(ctx *Modification_operationContext) {}

// ExitModification_operation is called when production modification_operation is exited.
func (s *BaseDb2ParserListener) ExitModification_operation(ctx *Modification_operationContext) {}

// EnterUpdate_operation is called when production update_operation is entered.
func (s *BaseDb2ParserListener) EnterUpdate_operation(ctx *Update_operationContext) {}

// ExitUpdate_operation is called when production update_operation is exited.
func (s *BaseDb2ParserListener) ExitUpdate_operation(ctx *Update_operationContext) {}

// EnterDelete_operation is called when production delete_operation is entered.
func (s *BaseDb2ParserListener) EnterDelete_operation(ctx *Delete_operationContext) {}

// ExitDelete_operation is called when production delete_operation is exited.
func (s *BaseDb2ParserListener) ExitDelete_operation(ctx *Delete_operationContext) {}

// EnterInsert_operation is called when production insert_operation is entered.
func (s *BaseDb2ParserListener) EnterInsert_operation(ctx *Insert_operationContext) {}

// ExitInsert_operation is called when production insert_operation is exited.
func (s *BaseDb2ParserListener) ExitInsert_operation(ctx *Insert_operationContext) {}

// EnterExpr_null_default_list is called when production expr_null_default_list is entered.
func (s *BaseDb2ParserListener) EnterExpr_null_default_list(ctx *Expr_null_default_listContext) {}

// ExitExpr_null_default_list is called when production expr_null_default_list is exited.
func (s *BaseDb2ParserListener) ExitExpr_null_default_list(ctx *Expr_null_default_listContext) {}

// EnterIsolation_level is called when production isolation_level is entered.
func (s *BaseDb2ParserListener) EnterIsolation_level(ctx *Isolation_levelContext) {}

// ExitIsolation_level is called when production isolation_level is exited.
func (s *BaseDb2ParserListener) ExitIsolation_level(ctx *Isolation_levelContext) {}

// EnterTruncate_statement is called when production truncate_statement is entered.
func (s *BaseDb2ParserListener) EnterTruncate_statement(ctx *Truncate_statementContext) {}

// ExitTruncate_statement is called when production truncate_statement is exited.
func (s *BaseDb2ParserListener) ExitTruncate_statement(ctx *Truncate_statementContext) {}

// EnterUpdate_statement is called when production update_statement is entered.
func (s *BaseDb2ParserListener) EnterUpdate_statement(ctx *Update_statementContext) {}

// ExitUpdate_statement is called when production update_statement is exited.
func (s *BaseDb2ParserListener) ExitUpdate_statement(ctx *Update_statementContext) {}

// EnterUpdate_statement_searched_update is called when production update_statement_searched_update is entered.
func (s *BaseDb2ParserListener) EnterUpdate_statement_searched_update(ctx *Update_statement_searched_updateContext) {
}

// ExitUpdate_statement_searched_update is called when production update_statement_searched_update is exited.
func (s *BaseDb2ParserListener) ExitUpdate_statement_searched_update(ctx *Update_statement_searched_updateContext) {
}

// EnterSkip_wait is called when production skip_wait is entered.
func (s *BaseDb2ParserListener) EnterSkip_wait(ctx *Skip_waitContext) {}

// ExitSkip_wait is called when production skip_wait is exited.
func (s *BaseDb2ParserListener) ExitSkip_wait(ctx *Skip_waitContext) {}

// EnterUpdate_statement_positioned_update is called when production update_statement_positioned_update is entered.
func (s *BaseDb2ParserListener) EnterUpdate_statement_positioned_update(ctx *Update_statement_positioned_updateContext) {
}

// ExitUpdate_statement_positioned_update is called when production update_statement_positioned_update is exited.
func (s *BaseDb2ParserListener) ExitUpdate_statement_positioned_update(ctx *Update_statement_positioned_updateContext) {
}

// EnterInclude_columns is called when production include_columns is entered.
func (s *BaseDb2ParserListener) EnterInclude_columns(ctx *Include_columnsContext) {}

// ExitInclude_columns is called when production include_columns is exited.
func (s *BaseDb2ParserListener) ExitInclude_columns(ctx *Include_columnsContext) {}

// EnterAssignment_clause is called when production assignment_clause is entered.
func (s *BaseDb2ParserListener) EnterAssignment_clause(ctx *Assignment_clauseContext) {}

// ExitAssignment_clause is called when production assignment_clause is exited.
func (s *BaseDb2ParserListener) ExitAssignment_clause(ctx *Assignment_clauseContext) {}

// EnterAssignment_item is called when production assignment_item is entered.
func (s *BaseDb2ParserListener) EnterAssignment_item(ctx *Assignment_itemContext) {}

// ExitAssignment_item is called when production assignment_item is exited.
func (s *BaseDb2ParserListener) ExitAssignment_item(ctx *Assignment_itemContext) {}

// EnterPeriod_clause is called when production period_clause is entered.
func (s *BaseDb2ParserListener) EnterPeriod_clause(ctx *Period_clauseContext) {}

// ExitPeriod_clause is called when production period_clause is exited.
func (s *BaseDb2ParserListener) ExitPeriod_clause(ctx *Period_clauseContext) {}

// EnterTime_sec is called when production time_sec is entered.
func (s *BaseDb2ParserListener) EnterTime_sec(ctx *Time_secContext) {}

// ExitTime_sec is called when production time_sec is exited.
func (s *BaseDb2ParserListener) ExitTime_sec(ctx *Time_secContext) {}

// EnterUpdate_datalake_statement is called when production update_datalake_statement is entered.
func (s *BaseDb2ParserListener) EnterUpdate_datalake_statement(ctx *Update_datalake_statementContext) {
}

// ExitUpdate_datalake_statement is called when production update_datalake_statement is exited.
func (s *BaseDb2ParserListener) ExitUpdate_datalake_statement(ctx *Update_datalake_statementContext) {
}

// EnterGrant_database_authorities_statement is called when production grant_database_authorities_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_database_authorities_statement(ctx *Grant_database_authorities_statementContext) {
}

// ExitGrant_database_authorities_statement is called when production grant_database_authorities_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_database_authorities_statement(ctx *Grant_database_authorities_statementContext) {
}

// EnterDb_privilege_list is called when production db_privilege_list is entered.
func (s *BaseDb2ParserListener) EnterDb_privilege_list(ctx *Db_privilege_listContext) {}

// ExitDb_privilege_list is called when production db_privilege_list is exited.
func (s *BaseDb2ParserListener) ExitDb_privilege_list(ctx *Db_privilege_listContext) {}

// EnterDb_privilege is called when production db_privilege is entered.
func (s *BaseDb2ParserListener) EnterDb_privilege(ctx *Db_privilegeContext) {}

// ExitDb_privilege is called when production db_privilege is exited.
func (s *BaseDb2ParserListener) ExitDb_privilege(ctx *Db_privilegeContext) {}

// EnterGrantee is called when production grantee is entered.
func (s *BaseDb2ParserListener) EnterGrantee(ctx *GranteeContext) {}

// ExitGrantee is called when production grantee is exited.
func (s *BaseDb2ParserListener) ExitGrantee(ctx *GranteeContext) {}

// EnterGrantee_user_group is called when production grantee_user_group is entered.
func (s *BaseDb2ParserListener) EnterGrantee_user_group(ctx *Grantee_user_groupContext) {}

// ExitGrantee_user_group is called when production grantee_user_group is exited.
func (s *BaseDb2ParserListener) ExitGrantee_user_group(ctx *Grantee_user_groupContext) {}

// EnterUser_group is called when production user_group is entered.
func (s *BaseDb2ParserListener) EnterUser_group(ctx *User_groupContext) {}

// ExitUser_group is called when production user_group is exited.
func (s *BaseDb2ParserListener) ExitUser_group(ctx *User_groupContext) {}

// EnterGrantee_list is called when production grantee_list is entered.
func (s *BaseDb2ParserListener) EnterGrantee_list(ctx *Grantee_listContext) {}

// ExitGrantee_list is called when production grantee_list is exited.
func (s *BaseDb2ParserListener) ExitGrantee_list(ctx *Grantee_listContext) {}

// EnterGrantee_list_public is called when production grantee_list_public is entered.
func (s *BaseDb2ParserListener) EnterGrantee_list_public(ctx *Grantee_list_publicContext) {}

// ExitGrantee_list_public is called when production grantee_list_public is exited.
func (s *BaseDb2ParserListener) ExitGrantee_list_public(ctx *Grantee_list_publicContext) {}

// EnterGrantee_list_user_group is called when production grantee_list_user_group is entered.
func (s *BaseDb2ParserListener) EnterGrantee_list_user_group(ctx *Grantee_list_user_groupContext) {}

// ExitGrantee_list_user_group is called when production grantee_list_user_group is exited.
func (s *BaseDb2ParserListener) ExitGrantee_list_user_group(ctx *Grantee_list_user_groupContext) {}

// EnterGrant_exemption_statement is called when production grant_exemption_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_exemption_statement(ctx *Grant_exemption_statementContext) {
}

// ExitGrant_exemption_statement is called when production grant_exemption_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_exemption_statement(ctx *Grant_exemption_statementContext) {
}

// EnterExemption_privilege is called when production exemption_privilege is entered.
func (s *BaseDb2ParserListener) EnterExemption_privilege(ctx *Exemption_privilegeContext) {}

// ExitExemption_privilege is called when production exemption_privilege is exited.
func (s *BaseDb2ParserListener) ExitExemption_privilege(ctx *Exemption_privilegeContext) {}

// EnterGrant_global_variable_privileges_statement is called when production grant_global_variable_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_global_variable_privileges_statement(ctx *Grant_global_variable_privileges_statementContext) {
}

// ExitGrant_global_variable_privileges_statement is called when production grant_global_variable_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_global_variable_privileges_statement(ctx *Grant_global_variable_privileges_statementContext) {
}

// EnterVariable_privilege is called when production variable_privilege is entered.
func (s *BaseDb2ParserListener) EnterVariable_privilege(ctx *Variable_privilegeContext) {}

// ExitVariable_privilege is called when production variable_privilege is exited.
func (s *BaseDb2ParserListener) ExitVariable_privilege(ctx *Variable_privilegeContext) {}

// EnterRead_write is called when production read_write is entered.
func (s *BaseDb2ParserListener) EnterRead_write(ctx *Read_writeContext) {}

// ExitRead_write is called when production read_write is exited.
func (s *BaseDb2ParserListener) ExitRead_write(ctx *Read_writeContext) {}

// EnterWith_grant_option is called when production with_grant_option is entered.
func (s *BaseDb2ParserListener) EnterWith_grant_option(ctx *With_grant_optionContext) {}

// ExitWith_grant_option is called when production with_grant_option is exited.
func (s *BaseDb2ParserListener) ExitWith_grant_option(ctx *With_grant_optionContext) {}

// EnterGrant_index_privileges_statement is called when production grant_index_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_index_privileges_statement(ctx *Grant_index_privileges_statementContext) {
}

// ExitGrant_index_privileges_statement is called when production grant_index_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_index_privileges_statement(ctx *Grant_index_privileges_statementContext) {
}

// EnterGrant_module_privileges_statement is called when production grant_module_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_module_privileges_statement(ctx *Grant_module_privileges_statementContext) {
}

// ExitGrant_module_privileges_statement is called when production grant_module_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_module_privileges_statement(ctx *Grant_module_privileges_statementContext) {
}

// EnterGrant_package_privileges_statement is called when production grant_package_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_package_privileges_statement(ctx *Grant_package_privileges_statementContext) {
}

// ExitGrant_package_privileges_statement is called when production grant_package_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_package_privileges_statement(ctx *Grant_package_privileges_statementContext) {
}

// EnterPackage_privilege_list is called when production package_privilege_list is entered.
func (s *BaseDb2ParserListener) EnterPackage_privilege_list(ctx *Package_privilege_listContext) {}

// ExitPackage_privilege_list is called when production package_privilege_list is exited.
func (s *BaseDb2ParserListener) ExitPackage_privilege_list(ctx *Package_privilege_listContext) {}

// EnterPackage_privilege is called when production package_privilege is entered.
func (s *BaseDb2ParserListener) EnterPackage_privilege(ctx *Package_privilegeContext) {}

// ExitPackage_privilege is called when production package_privilege is exited.
func (s *BaseDb2ParserListener) ExitPackage_privilege(ctx *Package_privilegeContext) {}

// EnterGrant_role_statement is called when production grant_role_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_role_statement(ctx *Grant_role_statementContext) {}

// ExitGrant_role_statement is called when production grant_role_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_role_statement(ctx *Grant_role_statementContext) {}

// EnterRole_list is called when production role_list is entered.
func (s *BaseDb2ParserListener) EnterRole_list(ctx *Role_listContext) {}

// ExitRole_list is called when production role_list is exited.
func (s *BaseDb2ParserListener) ExitRole_list(ctx *Role_listContext) {}

// EnterGrant_routine_privileges_statement is called when production grant_routine_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_routine_privileges_statement(ctx *Grant_routine_privileges_statementContext) {
}

// ExitGrant_routine_privileges_statement is called when production grant_routine_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_routine_privileges_statement(ctx *Grant_routine_privileges_statementContext) {
}

// EnterGrant_schema_privileges_statement is called when production grant_schema_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_schema_privileges_statement(ctx *Grant_schema_privileges_statementContext) {
}

// ExitGrant_schema_privileges_statement is called when production grant_schema_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_schema_privileges_statement(ctx *Grant_schema_privileges_statementContext) {
}

// EnterSchema_privilege_list is called when production schema_privilege_list is entered.
func (s *BaseDb2ParserListener) EnterSchema_privilege_list(ctx *Schema_privilege_listContext) {}

// ExitSchema_privilege_list is called when production schema_privilege_list is exited.
func (s *BaseDb2ParserListener) ExitSchema_privilege_list(ctx *Schema_privilege_listContext) {}

// EnterSchema_privilege is called when production schema_privilege is entered.
func (s *BaseDb2ParserListener) EnterSchema_privilege(ctx *Schema_privilegeContext) {}

// ExitSchema_privilege is called when production schema_privilege is exited.
func (s *BaseDb2ParserListener) ExitSchema_privilege(ctx *Schema_privilegeContext) {}

// EnterGrant_security_label_statement is called when production grant_security_label_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_security_label_statement(ctx *Grant_security_label_statementContext) {
}

// ExitGrant_security_label_statement is called when production grant_security_label_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_security_label_statement(ctx *Grant_security_label_statementContext) {
}

// EnterGrant_sequence_privileges_statement is called when production grant_sequence_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_sequence_privileges_statement(ctx *Grant_sequence_privileges_statementContext) {
}

// ExitGrant_sequence_privileges_statement is called when production grant_sequence_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_sequence_privileges_statement(ctx *Grant_sequence_privileges_statementContext) {
}

// EnterSequence_privilege_list is called when production sequence_privilege_list is entered.
func (s *BaseDb2ParserListener) EnterSequence_privilege_list(ctx *Sequence_privilege_listContext) {}

// ExitSequence_privilege_list is called when production sequence_privilege_list is exited.
func (s *BaseDb2ParserListener) ExitSequence_privilege_list(ctx *Sequence_privilege_listContext) {}

// EnterSequence_privilege is called when production sequence_privilege is entered.
func (s *BaseDb2ParserListener) EnterSequence_privilege(ctx *Sequence_privilegeContext) {}

// ExitSequence_privilege is called when production sequence_privilege is exited.
func (s *BaseDb2ParserListener) ExitSequence_privilege(ctx *Sequence_privilegeContext) {}

// EnterGrant_server_privileges_statement is called when production grant_server_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_server_privileges_statement(ctx *Grant_server_privileges_statementContext) {
}

// ExitGrant_server_privileges_statement is called when production grant_server_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_server_privileges_statement(ctx *Grant_server_privileges_statementContext) {
}

// EnterGrant_setsessionuser_privilege_statement is called when production grant_setsessionuser_privilege_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_setsessionuser_privilege_statement(ctx *Grant_setsessionuser_privilege_statementContext) {
}

// ExitGrant_setsessionuser_privilege_statement is called when production grant_setsessionuser_privilege_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_setsessionuser_privilege_statement(ctx *Grant_setsessionuser_privilege_statementContext) {
}

// EnterUser_list is called when production user_list is entered.
func (s *BaseDb2ParserListener) EnterUser_list(ctx *User_listContext) {}

// ExitUser_list is called when production user_list is exited.
func (s *BaseDb2ParserListener) ExitUser_list(ctx *User_listContext) {}

// EnterUser_auth is called when production user_auth is entered.
func (s *BaseDb2ParserListener) EnterUser_auth(ctx *User_authContext) {}

// ExitUser_auth is called when production user_auth is exited.
func (s *BaseDb2ParserListener) ExitUser_auth(ctx *User_authContext) {}

// EnterGrant_table_space_privileges_statement is called when production grant_table_space_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_table_space_privileges_statement(ctx *Grant_table_space_privileges_statementContext) {
}

// ExitGrant_table_space_privileges_statement is called when production grant_table_space_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_table_space_privileges_statement(ctx *Grant_table_space_privileges_statementContext) {
}

// EnterGrant_table_view_or_nickname_privileges_statement is called when production grant_table_view_or_nickname_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_table_view_or_nickname_privileges_statement(ctx *Grant_table_view_or_nickname_privileges_statementContext) {
}

// ExitGrant_table_view_or_nickname_privileges_statement is called when production grant_table_view_or_nickname_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_table_view_or_nickname_privileges_statement(ctx *Grant_table_view_or_nickname_privileges_statementContext) {
}

// EnterTvn_privilege_list is called when production tvn_privilege_list is entered.
func (s *BaseDb2ParserListener) EnterTvn_privilege_list(ctx *Tvn_privilege_listContext) {}

// ExitTvn_privilege_list is called when production tvn_privilege_list is exited.
func (s *BaseDb2ParserListener) ExitTvn_privilege_list(ctx *Tvn_privilege_listContext) {}

// EnterTvn_privilege is called when production tvn_privilege is entered.
func (s *BaseDb2ParserListener) EnterTvn_privilege(ctx *Tvn_privilegeContext) {}

// ExitTvn_privilege is called when production tvn_privilege is exited.
func (s *BaseDb2ParserListener) ExitTvn_privilege(ctx *Tvn_privilegeContext) {}

// EnterColumn_name_list_paren is called when production column_name_list_paren is entered.
func (s *BaseDb2ParserListener) EnterColumn_name_list_paren(ctx *Column_name_list_parenContext) {}

// ExitColumn_name_list_paren is called when production column_name_list_paren is exited.
func (s *BaseDb2ParserListener) ExitColumn_name_list_paren(ctx *Column_name_list_parenContext) {}

// EnterColumn_name_list is called when production column_name_list is entered.
func (s *BaseDb2ParserListener) EnterColumn_name_list(ctx *Column_name_listContext) {}

// ExitColumn_name_list is called when production column_name_list is exited.
func (s *BaseDb2ParserListener) ExitColumn_name_list(ctx *Column_name_listContext) {}

// EnterGrant_workload_privileges_statement is called when production grant_workload_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_workload_privileges_statement(ctx *Grant_workload_privileges_statementContext) {
}

// ExitGrant_workload_privileges_statement is called when production grant_workload_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_workload_privileges_statement(ctx *Grant_workload_privileges_statementContext) {
}

// EnterGrant_xsr_object_privileges_statement is called when production grant_xsr_object_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterGrant_xsr_object_privileges_statement(ctx *Grant_xsr_object_privileges_statementContext) {
}

// ExitGrant_xsr_object_privileges_statement is called when production grant_xsr_object_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitGrant_xsr_object_privileges_statement(ctx *Grant_xsr_object_privileges_statementContext) {
}

// EnterRevoke_database_authorities_statement is called when production revoke_database_authorities_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_database_authorities_statement(ctx *Revoke_database_authorities_statementContext) {
}

// ExitRevoke_database_authorities_statement is called when production revoke_database_authorities_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_database_authorities_statement(ctx *Revoke_database_authorities_statementContext) {
}

// EnterBy_all is called when production by_all is entered.
func (s *BaseDb2ParserListener) EnterBy_all(ctx *By_allContext) {}

// ExitBy_all is called when production by_all is exited.
func (s *BaseDb2ParserListener) ExitBy_all(ctx *By_allContext) {}

// EnterRevoke_exemption_statement is called when production revoke_exemption_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_exemption_statement(ctx *Revoke_exemption_statementContext) {
}

// ExitRevoke_exemption_statement is called when production revoke_exemption_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_exemption_statement(ctx *Revoke_exemption_statementContext) {
}

// EnterRevoke_global_variable_privileges_statement is called when production revoke_global_variable_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_global_variable_privileges_statement(ctx *Revoke_global_variable_privileges_statementContext) {
}

// ExitRevoke_global_variable_privileges_statement is called when production revoke_global_variable_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_global_variable_privileges_statement(ctx *Revoke_global_variable_privileges_statementContext) {
}

// EnterRevoke_index_privileges_statement is called when production revoke_index_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_index_privileges_statement(ctx *Revoke_index_privileges_statementContext) {
}

// ExitRevoke_index_privileges_statement is called when production revoke_index_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_index_privileges_statement(ctx *Revoke_index_privileges_statementContext) {
}

// EnterRevoke_module_privileges_statement is called when production revoke_module_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_module_privileges_statement(ctx *Revoke_module_privileges_statementContext) {
}

// ExitRevoke_module_privileges_statement is called when production revoke_module_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_module_privileges_statement(ctx *Revoke_module_privileges_statementContext) {
}

// EnterRevoke_package_privileges_statement is called when production revoke_package_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_package_privileges_statement(ctx *Revoke_package_privileges_statementContext) {
}

// ExitRevoke_package_privileges_statement is called when production revoke_package_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_package_privileges_statement(ctx *Revoke_package_privileges_statementContext) {
}

// EnterRevoke_role_statement is called when production revoke_role_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_role_statement(ctx *Revoke_role_statementContext) {}

// ExitRevoke_role_statement is called when production revoke_role_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_role_statement(ctx *Revoke_role_statementContext) {}

// EnterRevoke_routine_privileges_statement is called when production revoke_routine_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_routine_privileges_statement(ctx *Revoke_routine_privileges_statementContext) {
}

// ExitRevoke_routine_privileges_statement is called when production revoke_routine_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_routine_privileges_statement(ctx *Revoke_routine_privileges_statementContext) {
}

// EnterRevoke_schema_privileges_statement is called when production revoke_schema_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_schema_privileges_statement(ctx *Revoke_schema_privileges_statementContext) {
}

// ExitRevoke_schema_privileges_statement is called when production revoke_schema_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_schema_privileges_statement(ctx *Revoke_schema_privileges_statementContext) {
}

// EnterRevoke_security_label_statement is called when production revoke_security_label_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_security_label_statement(ctx *Revoke_security_label_statementContext) {
}

// ExitRevoke_security_label_statement is called when production revoke_security_label_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_security_label_statement(ctx *Revoke_security_label_statementContext) {
}

// EnterRevoke_sequence_privileges_statement is called when production revoke_sequence_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_sequence_privileges_statement(ctx *Revoke_sequence_privileges_statementContext) {
}

// ExitRevoke_sequence_privileges_statement is called when production revoke_sequence_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_sequence_privileges_statement(ctx *Revoke_sequence_privileges_statementContext) {
}

// EnterRevoke_server_privileges_statement is called when production revoke_server_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_server_privileges_statement(ctx *Revoke_server_privileges_statementContext) {
}

// ExitRevoke_server_privileges_statement is called when production revoke_server_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_server_privileges_statement(ctx *Revoke_server_privileges_statementContext) {
}

// EnterRevoke_setsessionuser_privilege_statement is called when production revoke_setsessionuser_privilege_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_setsessionuser_privilege_statement(ctx *Revoke_setsessionuser_privilege_statementContext) {
}

// ExitRevoke_setsessionuser_privilege_statement is called when production revoke_setsessionuser_privilege_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_setsessionuser_privilege_statement(ctx *Revoke_setsessionuser_privilege_statementContext) {
}

// EnterRevoke_table_space_privileges_statement is called when production revoke_table_space_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_table_space_privileges_statement(ctx *Revoke_table_space_privileges_statementContext) {
}

// ExitRevoke_table_space_privileges_statement is called when production revoke_table_space_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_table_space_privileges_statement(ctx *Revoke_table_space_privileges_statementContext) {
}

// EnterRevoke_table_view_or_nickname_privileges_statement is called when production revoke_table_view_or_nickname_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_table_view_or_nickname_privileges_statement(ctx *Revoke_table_view_or_nickname_privileges_statementContext) {
}

// ExitRevoke_table_view_or_nickname_privileges_statement is called when production revoke_table_view_or_nickname_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_table_view_or_nickname_privileges_statement(ctx *Revoke_table_view_or_nickname_privileges_statementContext) {
}

// EnterRevoke_workload_privileges_statement is called when production revoke_workload_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_workload_privileges_statement(ctx *Revoke_workload_privileges_statementContext) {
}

// ExitRevoke_workload_privileges_statement is called when production revoke_workload_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_workload_privileges_statement(ctx *Revoke_workload_privileges_statementContext) {
}

// EnterRevoke_xsr_object_privileges_statement is called when production revoke_xsr_object_privileges_statement is entered.
func (s *BaseDb2ParserListener) EnterRevoke_xsr_object_privileges_statement(ctx *Revoke_xsr_object_privileges_statementContext) {
}

// ExitRevoke_xsr_object_privileges_statement is called when production revoke_xsr_object_privileges_statement is exited.
func (s *BaseDb2ParserListener) ExitRevoke_xsr_object_privileges_statement(ctx *Revoke_xsr_object_privileges_statementContext) {
}

// EnterUser_group_role is called when production user_group_role is entered.
func (s *BaseDb2ParserListener) EnterUser_group_role(ctx *User_group_roleContext) {}

// ExitUser_group_role is called when production user_group_role is exited.
func (s *BaseDb2ParserListener) ExitUser_group_role(ctx *User_group_roleContext) {}

// EnterRollback_statement is called when production rollback_statement is entered.
func (s *BaseDb2ParserListener) EnterRollback_statement(ctx *Rollback_statementContext) {}

// ExitRollback_statement is called when production rollback_statement is exited.
func (s *BaseDb2ParserListener) ExitRollback_statement(ctx *Rollback_statementContext) {}

// EnterSavepoint_statement is called when production savepoint_statement is entered.
func (s *BaseDb2ParserListener) EnterSavepoint_statement(ctx *Savepoint_statementContext) {}

// ExitSavepoint_statement is called when production savepoint_statement is exited.
func (s *BaseDb2ParserListener) ExitSavepoint_statement(ctx *Savepoint_statementContext) {}

// EnterRelease_savepoint_statement is called when production release_savepoint_statement is entered.
func (s *BaseDb2ParserListener) EnterRelease_savepoint_statement(ctx *Release_savepoint_statementContext) {
}

// ExitRelease_savepoint_statement is called when production release_savepoint_statement is exited.
func (s *BaseDb2ParserListener) ExitRelease_savepoint_statement(ctx *Release_savepoint_statementContext) {
}

// EnterAllocate_cursor_statement is called when production allocate_cursor_statement is entered.
func (s *BaseDb2ParserListener) EnterAllocate_cursor_statement(ctx *Allocate_cursor_statementContext) {
}

// ExitAllocate_cursor_statement is called when production allocate_cursor_statement is exited.
func (s *BaseDb2ParserListener) ExitAllocate_cursor_statement(ctx *Allocate_cursor_statementContext) {
}

// EnterAlter_audit_policy_statement is called when production alter_audit_policy_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_audit_policy_statement(ctx *Alter_audit_policy_statementContext) {
}

// ExitAlter_audit_policy_statement is called when production alter_audit_policy_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_audit_policy_statement(ctx *Alter_audit_policy_statementContext) {
}

// EnterStatus_spec is called when production status_spec is entered.
func (s *BaseDb2ParserListener) EnterStatus_spec(ctx *Status_specContext) {}

// ExitStatus_spec is called when production status_spec is exited.
func (s *BaseDb2ParserListener) ExitStatus_spec(ctx *Status_specContext) {}

// EnterNormal_audit is called when production normal_audit is entered.
func (s *BaseDb2ParserListener) EnterNormal_audit(ctx *Normal_auditContext) {}

// ExitNormal_audit is called when production normal_audit is exited.
func (s *BaseDb2ParserListener) ExitNormal_audit(ctx *Normal_auditContext) {}

// EnterAlter_bufferpool_statement is called when production alter_bufferpool_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_bufferpool_statement(ctx *Alter_bufferpool_statementContext) {
}

// ExitAlter_bufferpool_statement is called when production alter_bufferpool_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_bufferpool_statement(ctx *Alter_bufferpool_statementContext) {
}

// EnterImmediate_deferred is called when production immediate_deferred is entered.
func (s *BaseDb2ParserListener) EnterImmediate_deferred(ctx *Immediate_deferredContext) {}

// ExitImmediate_deferred is called when production immediate_deferred is exited.
func (s *BaseDb2ParserListener) ExitImmediate_deferred(ctx *Immediate_deferredContext) {}

// EnterAlter_database_partition_group_statement is called when production alter_database_partition_group_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_database_partition_group_statement(ctx *Alter_database_partition_group_statementContext) {
}

// ExitAlter_database_partition_group_statement is called when production alter_database_partition_group_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_database_partition_group_statement(ctx *Alter_database_partition_group_statementContext) {
}

// EnterDb_partition_group_list_item is called when production db_partition_group_list_item is entered.
func (s *BaseDb2ParserListener) EnterDb_partition_group_list_item(ctx *Db_partition_group_list_itemContext) {
}

// ExitDb_partition_group_list_item is called when production db_partition_group_list_item is exited.
func (s *BaseDb2ParserListener) ExitDb_partition_group_list_item(ctx *Db_partition_group_list_itemContext) {
}

// EnterDb_partition_num_nums is called when production db_partition_num_nums is entered.
func (s *BaseDb2ParserListener) EnterDb_partition_num_nums(ctx *Db_partition_num_numsContext) {}

// ExitDb_partition_num_nums is called when production db_partition_num_nums is exited.
func (s *BaseDb2ParserListener) ExitDb_partition_num_nums(ctx *Db_partition_num_numsContext) {}

// EnterDb_partitions_clause is called when production db_partitions_clause is entered.
func (s *BaseDb2ParserListener) EnterDb_partitions_clause(ctx *Db_partitions_clauseContext) {}

// ExitDb_partitions_clause is called when production db_partitions_clause is exited.
func (s *BaseDb2ParserListener) ExitDb_partitions_clause(ctx *Db_partitions_clauseContext) {}

// EnterDb_partition_options is called when production db_partition_options is entered.
func (s *BaseDb2ParserListener) EnterDb_partition_options(ctx *Db_partition_optionsContext) {}

// ExitDb_partition_options is called when production db_partition_options is exited.
func (s *BaseDb2ParserListener) ExitDb_partition_options(ctx *Db_partition_optionsContext) {}

// EnterAlter_database_statement is called when production alter_database_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_database_statement(ctx *Alter_database_statementContext) {}

// ExitAlter_database_statement is called when production alter_database_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_database_statement(ctx *Alter_database_statementContext) {}

// EnterAlter_database_opts is called when production alter_database_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_database_opts(ctx *Alter_database_optsContext) {}

// ExitAlter_database_opts is called when production alter_database_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_database_opts(ctx *Alter_database_optsContext) {}

// EnterAlter_event_monitor_statement is called when production alter_event_monitor_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_event_monitor_statement(ctx *Alter_event_monitor_statementContext) {
}

// ExitAlter_event_monitor_statement is called when production alter_event_monitor_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_event_monitor_statement(ctx *Alter_event_monitor_statementContext) {
}

// EnterAlter_event_monitor_opts is called when production alter_event_monitor_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_event_monitor_opts(ctx *Alter_event_monitor_optsContext) {}

// ExitAlter_event_monitor_opts is called when production alter_event_monitor_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_event_monitor_opts(ctx *Alter_event_monitor_optsContext) {}

// EnterAlter_function_statement is called when production alter_function_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_function_statement(ctx *Alter_function_statementContext) {}

// ExitAlter_function_statement is called when production alter_function_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_function_statement(ctx *Alter_function_statementContext) {}

// EnterAlter_function_opts is called when production alter_function_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_function_opts(ctx *Alter_function_optsContext) {}

// ExitAlter_function_opts is called when production alter_function_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_function_opts(ctx *Alter_function_optsContext) {}

// EnterFunction_designator is called when production function_designator is entered.
func (s *BaseDb2ParserListener) EnterFunction_designator(ctx *Function_designatorContext) {}

// ExitFunction_designator is called when production function_designator is exited.
func (s *BaseDb2ParserListener) ExitFunction_designator(ctx *Function_designatorContext) {}

// EnterData_type_list is called when production data_type_list is entered.
func (s *BaseDb2ParserListener) EnterData_type_list(ctx *Data_type_listContext) {}

// ExitData_type_list is called when production data_type_list is exited.
func (s *BaseDb2ParserListener) ExitData_type_list(ctx *Data_type_listContext) {}

// EnterData_type_list_paren is called when production data_type_list_paren is entered.
func (s *BaseDb2ParserListener) EnterData_type_list_paren(ctx *Data_type_list_parenContext) {}

// ExitData_type_list_paren is called when production data_type_list_paren is exited.
func (s *BaseDb2ParserListener) ExitData_type_list_paren(ctx *Data_type_list_parenContext) {}

// EnterAlter_histogram_template_statement is called when production alter_histogram_template_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_histogram_template_statement(ctx *Alter_histogram_template_statementContext) {
}

// ExitAlter_histogram_template_statement is called when production alter_histogram_template_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_histogram_template_statement(ctx *Alter_histogram_template_statementContext) {
}

// EnterAlter_index_statement is called when production alter_index_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_index_statement(ctx *Alter_index_statementContext) {}

// ExitAlter_index_statement is called when production alter_index_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_index_statement(ctx *Alter_index_statementContext) {}

// EnterYes_no is called when production yes_no is entered.
func (s *BaseDb2ParserListener) EnterYes_no(ctx *Yes_noContext) {}

// ExitYes_no is called when production yes_no is exited.
func (s *BaseDb2ParserListener) ExitYes_no(ctx *Yes_noContext) {}

// EnterAlter_mask_statement is called when production alter_mask_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_mask_statement(ctx *Alter_mask_statementContext) {}

// ExitAlter_mask_statement is called when production alter_mask_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_mask_statement(ctx *Alter_mask_statementContext) {}

// EnterEnable_disable is called when production enable_disable is entered.
func (s *BaseDb2ParserListener) EnterEnable_disable(ctx *Enable_disableContext) {}

// ExitEnable_disable is called when production enable_disable is exited.
func (s *BaseDb2ParserListener) ExitEnable_disable(ctx *Enable_disableContext) {}

// EnterAlter_method_statement is called when production alter_method_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_method_statement(ctx *Alter_method_statementContext) {}

// ExitAlter_method_statement is called when production alter_method_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_method_statement(ctx *Alter_method_statementContext) {}

// EnterMethod_designator is called when production method_designator is entered.
func (s *BaseDb2ParserListener) EnterMethod_designator(ctx *Method_designatorContext) {}

// ExitMethod_designator is called when production method_designator is exited.
func (s *BaseDb2ParserListener) ExitMethod_designator(ctx *Method_designatorContext) {}

// EnterAlter_model_statement is called when production alter_model_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_model_statement(ctx *Alter_model_statementContext) {}

// ExitAlter_model_statement is called when production alter_model_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_model_statement(ctx *Alter_model_statementContext) {}

// EnterAlter_module_statement is called when production alter_module_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_module_statement(ctx *Alter_module_statementContext) {}

// ExitAlter_module_statement is called when production alter_module_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_module_statement(ctx *Alter_module_statementContext) {}

// EnterAlter_module_opts is called when production alter_module_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_module_opts(ctx *Alter_module_optsContext) {}

// ExitAlter_module_opts is called when production alter_module_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_module_opts(ctx *Alter_module_optsContext) {}

// EnterModule_function_definition is called when production module_function_definition is entered.
func (s *BaseDb2ParserListener) EnterModule_function_definition(ctx *Module_function_definitionContext) {
}

// ExitModule_function_definition is called when production module_function_definition is exited.
func (s *BaseDb2ParserListener) ExitModule_function_definition(ctx *Module_function_definitionContext) {
}

// EnterModule_procedure_definition is called when production module_procedure_definition is entered.
func (s *BaseDb2ParserListener) EnterModule_procedure_definition(ctx *Module_procedure_definitionContext) {
}

// ExitModule_procedure_definition is called when production module_procedure_definition is exited.
func (s *BaseDb2ParserListener) ExitModule_procedure_definition(ctx *Module_procedure_definitionContext) {
}

// EnterModule_type_definition is called when production module_type_definition is entered.
func (s *BaseDb2ParserListener) EnterModule_type_definition(ctx *Module_type_definitionContext) {}

// ExitModule_type_definition is called when production module_type_definition is exited.
func (s *BaseDb2ParserListener) ExitModule_type_definition(ctx *Module_type_definitionContext) {}

// EnterModule_variable_definition is called when production module_variable_definition is entered.
func (s *BaseDb2ParserListener) EnterModule_variable_definition(ctx *Module_variable_definitionContext) {
}

// ExitModule_variable_definition is called when production module_variable_definition is exited.
func (s *BaseDb2ParserListener) ExitModule_variable_definition(ctx *Module_variable_definitionContext) {
}

// EnterModule_condition_definition is called when production module_condition_definition is entered.
func (s *BaseDb2ParserListener) EnterModule_condition_definition(ctx *Module_condition_definitionContext) {
}

// ExitModule_condition_definition is called when production module_condition_definition is exited.
func (s *BaseDb2ParserListener) ExitModule_condition_definition(ctx *Module_condition_definitionContext) {
}

// EnterModule_object_identification is called when production module_object_identification is entered.
func (s *BaseDb2ParserListener) EnterModule_object_identification(ctx *Module_object_identificationContext) {
}

// ExitModule_object_identification is called when production module_object_identification is exited.
func (s *BaseDb2ParserListener) ExitModule_object_identification(ctx *Module_object_identificationContext) {
}

// EnterModule_function_designator is called when production module_function_designator is entered.
func (s *BaseDb2ParserListener) EnterModule_function_designator(ctx *Module_function_designatorContext) {
}

// ExitModule_function_designator is called when production module_function_designator is exited.
func (s *BaseDb2ParserListener) ExitModule_function_designator(ctx *Module_function_designatorContext) {
}

// EnterModule_procedure_designator is called when production module_procedure_designator is entered.
func (s *BaseDb2ParserListener) EnterModule_procedure_designator(ctx *Module_procedure_designatorContext) {
}

// ExitModule_procedure_designator is called when production module_procedure_designator is exited.
func (s *BaseDb2ParserListener) ExitModule_procedure_designator(ctx *Module_procedure_designatorContext) {
}

// EnterAlter_nickname_statement is called when production alter_nickname_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_nickname_statement(ctx *Alter_nickname_statementContext) {}

// ExitAlter_nickname_statement is called when production alter_nickname_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_nickname_statement(ctx *Alter_nickname_statementContext) {}

// EnterAlter_nickname_opts_1 is called when production alter_nickname_opts_1 is entered.
func (s *BaseDb2ParserListener) EnterAlter_nickname_opts_1(ctx *Alter_nickname_opts_1Context) {}

// ExitAlter_nickname_opts_1 is called when production alter_nickname_opts_1 is exited.
func (s *BaseDb2ParserListener) ExitAlter_nickname_opts_1(ctx *Alter_nickname_opts_1Context) {}

// EnterAlter_nickname_opts_1_item is called when production alter_nickname_opts_1_item is entered.
func (s *BaseDb2ParserListener) EnterAlter_nickname_opts_1_item(ctx *Alter_nickname_opts_1_itemContext) {
}

// ExitAlter_nickname_opts_1_item is called when production alter_nickname_opts_1_item is exited.
func (s *BaseDb2ParserListener) ExitAlter_nickname_opts_1_item(ctx *Alter_nickname_opts_1_itemContext) {
}

// EnterAlter_nickname_opts_2 is called when production alter_nickname_opts_2 is entered.
func (s *BaseDb2ParserListener) EnterAlter_nickname_opts_2(ctx *Alter_nickname_opts_2Context) {}

// ExitAlter_nickname_opts_2 is called when production alter_nickname_opts_2 is exited.
func (s *BaseDb2ParserListener) ExitAlter_nickname_opts_2(ctx *Alter_nickname_opts_2Context) {}

// EnterAlter_nickname_opts_2_item is called when production alter_nickname_opts_2_item is entered.
func (s *BaseDb2ParserListener) EnterAlter_nickname_opts_2_item(ctx *Alter_nickname_opts_2_itemContext) {
}

// ExitAlter_nickname_opts_2_item is called when production alter_nickname_opts_2_item is exited.
func (s *BaseDb2ParserListener) ExitAlter_nickname_opts_2_item(ctx *Alter_nickname_opts_2_itemContext) {
}

// EnterConstraint_alteration is called when production constraint_alteration is entered.
func (s *BaseDb2ParserListener) EnterConstraint_alteration(ctx *Constraint_alterationContext) {}

// ExitConstraint_alteration is called when production constraint_alteration is exited.
func (s *BaseDb2ParserListener) ExitConstraint_alteration(ctx *Constraint_alterationContext) {}

// EnterAlter_package_statement is called when production alter_package_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_package_statement(ctx *Alter_package_statementContext) {}

// ExitAlter_package_statement is called when production alter_package_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_package_statement(ctx *Alter_package_statementContext) {}

// EnterAlter_package_opts is called when production alter_package_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_package_opts(ctx *Alter_package_optsContext) {}

// ExitAlter_package_opts is called when production alter_package_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_package_opts(ctx *Alter_package_optsContext) {}

// EnterAlter_permission_statement is called when production alter_permission_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_permission_statement(ctx *Alter_permission_statementContext) {
}

// ExitAlter_permission_statement is called when production alter_permission_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_permission_statement(ctx *Alter_permission_statementContext) {
}

// EnterAlter_procedure_external_statement is called when production alter_procedure_external_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_procedure_external_statement(ctx *Alter_procedure_external_statementContext) {
}

// ExitAlter_procedure_external_statement is called when production alter_procedure_external_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_procedure_external_statement(ctx *Alter_procedure_external_statementContext) {
}

// EnterAlter_procedure_external_opts is called when production alter_procedure_external_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_procedure_external_opts(ctx *Alter_procedure_external_optsContext) {
}

// ExitAlter_procedure_external_opts is called when production alter_procedure_external_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_procedure_external_opts(ctx *Alter_procedure_external_optsContext) {
}

// EnterProcedure_designator is called when production procedure_designator is entered.
func (s *BaseDb2ParserListener) EnterProcedure_designator(ctx *Procedure_designatorContext) {}

// ExitProcedure_designator is called when production procedure_designator is exited.
func (s *BaseDb2ParserListener) ExitProcedure_designator(ctx *Procedure_designatorContext) {}

// EnterAlter_procedure_sourced_statement is called when production alter_procedure_sourced_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_procedure_sourced_statement(ctx *Alter_procedure_sourced_statementContext) {
}

// ExitAlter_procedure_sourced_statement is called when production alter_procedure_sourced_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_procedure_sourced_statement(ctx *Alter_procedure_sourced_statementContext) {
}

// EnterParameter_alteration is called when production parameter_alteration is entered.
func (s *BaseDb2ParserListener) EnterParameter_alteration(ctx *Parameter_alterationContext) {}

// ExitParameter_alteration is called when production parameter_alteration is exited.
func (s *BaseDb2ParserListener) ExitParameter_alteration(ctx *Parameter_alterationContext) {}

// EnterAlter_procedure_sql_statement is called when production alter_procedure_sql_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_procedure_sql_statement(ctx *Alter_procedure_sql_statementContext) {
}

// ExitAlter_procedure_sql_statement is called when production alter_procedure_sql_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_procedure_sql_statement(ctx *Alter_procedure_sql_statementContext) {
}

// EnterAlter_schema_statement is called when production alter_schema_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_schema_statement(ctx *Alter_schema_statementContext) {}

// ExitAlter_schema_statement is called when production alter_schema_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_schema_statement(ctx *Alter_schema_statementContext) {}

// EnterNone_changes is called when production none_changes is entered.
func (s *BaseDb2ParserListener) EnterNone_changes(ctx *None_changesContext) {}

// ExitNone_changes is called when production none_changes is exited.
func (s *BaseDb2ParserListener) ExitNone_changes(ctx *None_changesContext) {}

// EnterAlter_security_label_component_statement is called when production alter_security_label_component_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_security_label_component_statement(ctx *Alter_security_label_component_statementContext) {
}

// ExitAlter_security_label_component_statement is called when production alter_security_label_component_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_security_label_component_statement(ctx *Alter_security_label_component_statementContext) {
}

// EnterAdd_element_clause is called when production add_element_clause is entered.
func (s *BaseDb2ParserListener) EnterAdd_element_clause(ctx *Add_element_clauseContext) {}

// ExitAdd_element_clause is called when production add_element_clause is exited.
func (s *BaseDb2ParserListener) ExitAdd_element_clause(ctx *Add_element_clauseContext) {}

// EnterArray_element_clause is called when production array_element_clause is entered.
func (s *BaseDb2ParserListener) EnterArray_element_clause(ctx *Array_element_clauseContext) {}

// ExitArray_element_clause is called when production array_element_clause is exited.
func (s *BaseDb2ParserListener) ExitArray_element_clause(ctx *Array_element_clauseContext) {}

// EnterTree_element_clause is called when production tree_element_clause is entered.
func (s *BaseDb2ParserListener) EnterTree_element_clause(ctx *Tree_element_clauseContext) {}

// ExitTree_element_clause is called when production tree_element_clause is exited.
func (s *BaseDb2ParserListener) ExitTree_element_clause(ctx *Tree_element_clauseContext) {}

// EnterAlter_security_policy_statement is called when production alter_security_policy_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_security_policy_statement(ctx *Alter_security_policy_statementContext) {
}

// ExitAlter_security_policy_statement is called when production alter_security_policy_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_security_policy_statement(ctx *Alter_security_policy_statementContext) {
}

// EnterAlter_security_policy_opts is called when production alter_security_policy_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_security_policy_opts(ctx *Alter_security_policy_optsContext) {
}

// ExitAlter_security_policy_opts is called when production alter_security_policy_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_security_policy_opts(ctx *Alter_security_policy_optsContext) {
}

// EnterAlter_sequence_statement is called when production alter_sequence_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_sequence_statement(ctx *Alter_sequence_statementContext) {}

// ExitAlter_sequence_statement is called when production alter_sequence_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_sequence_statement(ctx *Alter_sequence_statementContext) {}

// EnterAlter_sequence_opts is called when production alter_sequence_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_sequence_opts(ctx *Alter_sequence_optsContext) {}

// ExitAlter_sequence_opts is called when production alter_sequence_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_sequence_opts(ctx *Alter_sequence_optsContext) {}

// EnterAlter_server_statement is called when production alter_server_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_server_statement(ctx *Alter_server_statementContext) {}

// ExitAlter_server_statement is called when production alter_server_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_server_statement(ctx *Alter_server_statementContext) {}

// EnterAlter_server_opts is called when production alter_server_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_server_opts(ctx *Alter_server_optsContext) {}

// ExitAlter_server_opts is called when production alter_server_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_server_opts(ctx *Alter_server_optsContext) {}

// EnterAlter_service_class_statement is called when production alter_service_class_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_service_class_statement(ctx *Alter_service_class_statementContext) {
}

// ExitAlter_service_class_statement is called when production alter_service_class_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_service_class_statement(ctx *Alter_service_class_statementContext) {
}

// EnterAlter_service_class_opts is called when production alter_service_class_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_service_class_opts(ctx *Alter_service_class_optsContext) {}

// ExitAlter_service_class_opts is called when production alter_service_class_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_service_class_opts(ctx *Alter_service_class_optsContext) {}

// EnterDefault_on_off is called when production default_on_off is entered.
func (s *BaseDb2ParserListener) EnterDefault_on_off(ctx *Default_on_offContext) {}

// ExitDefault_on_off is called when production default_on_off is exited.
func (s *BaseDb2ParserListener) ExitDefault_on_off(ctx *Default_on_offContext) {}

// EnterDefault_high_medium_low is called when production default_high_medium_low is entered.
func (s *BaseDb2ParserListener) EnterDefault_high_medium_low(ctx *Default_high_medium_lowContext) {}

// ExitDefault_high_medium_low is called when production default_high_medium_low is exited.
func (s *BaseDb2ParserListener) ExitDefault_high_medium_low(ctx *Default_high_medium_lowContext) {}

// EnterAlter_stogroup_statement is called when production alter_stogroup_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_stogroup_statement(ctx *Alter_stogroup_statementContext) {}

// ExitAlter_stogroup_statement is called when production alter_stogroup_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_stogroup_statement(ctx *Alter_stogroup_statementContext) {}

// EnterAlter_stogroup_opts is called when production alter_stogroup_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_stogroup_opts(ctx *Alter_stogroup_optsContext) {}

// ExitAlter_stogroup_opts is called when production alter_stogroup_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_stogroup_opts(ctx *Alter_stogroup_optsContext) {}

// EnterAlter_table_statement is called when production alter_table_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_table_statement(ctx *Alter_table_statementContext) {}

// ExitAlter_table_statement is called when production alter_table_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_table_statement(ctx *Alter_table_statementContext) {}

// EnterAlter_table_opts is called when production alter_table_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_table_opts(ctx *Alter_table_optsContext) {}

// ExitAlter_table_opts is called when production alter_table_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_table_opts(ctx *Alter_table_optsContext) {}

// EnterNull_on_off is called when production null_on_off is entered.
func (s *BaseDb2ParserListener) EnterNull_on_off(ctx *Null_on_offContext) {}

// ExitNull_on_off is called when production null_on_off is exited.
func (s *BaseDb2ParserListener) ExitNull_on_off(ctx *Null_on_offContext) {}

// EnterCascade_restrict is called when production cascade_restrict is entered.
func (s *BaseDb2ParserListener) EnterCascade_restrict(ctx *Cascade_restrictContext) {}

// ExitCascade_restrict is called when production cascade_restrict is exited.
func (s *BaseDb2ParserListener) ExitCascade_restrict(ctx *Cascade_restrictContext) {}

// EnterMaterialized_query_definition is called when production materialized_query_definition is entered.
func (s *BaseDb2ParserListener) EnterMaterialized_query_definition(ctx *Materialized_query_definitionContext) {
}

// ExitMaterialized_query_definition is called when production materialized_query_definition is exited.
func (s *BaseDb2ParserListener) ExitMaterialized_query_definition(ctx *Materialized_query_definitionContext) {
}

// EnterRefreshable_table_options is called when production refreshable_table_options is entered.
func (s *BaseDb2ParserListener) EnterRefreshable_table_options(ctx *Refreshable_table_optionsContext) {
}

// ExitRefreshable_table_options is called when production refreshable_table_options is exited.
func (s *BaseDb2ParserListener) ExitRefreshable_table_options(ctx *Refreshable_table_optionsContext) {
}

// EnterColumn_alteration is called when production column_alteration is entered.
func (s *BaseDb2ParserListener) EnterColumn_alteration(ctx *Column_alterationContext) {}

// ExitColumn_alteration is called when production column_alteration is exited.
func (s *BaseDb2ParserListener) ExitColumn_alteration(ctx *Column_alterationContext) {}

// EnterGeneration_alteration is called when production generation_alteration is entered.
func (s *BaseDb2ParserListener) EnterGeneration_alteration(ctx *Generation_alterationContext) {}

// ExitGeneration_alteration is called when production generation_alteration is exited.
func (s *BaseDb2ParserListener) ExitGeneration_alteration(ctx *Generation_alterationContext) {}

// EnterIdentity_alteration is called when production identity_alteration is entered.
func (s *BaseDb2ParserListener) EnterIdentity_alteration(ctx *Identity_alterationContext) {}

// ExitIdentity_alteration is called when production identity_alteration is exited.
func (s *BaseDb2ParserListener) ExitIdentity_alteration(ctx *Identity_alterationContext) {}

// EnterGeneration_attribute is called when production generation_attribute is entered.
func (s *BaseDb2ParserListener) EnterGeneration_attribute(ctx *Generation_attributeContext) {}

// ExitGeneration_attribute is called when production generation_attribute is exited.
func (s *BaseDb2ParserListener) ExitGeneration_attribute(ctx *Generation_attributeContext) {}

// EnterAs_identity_clause is called when production as_identity_clause is entered.
func (s *BaseDb2ParserListener) EnterAs_identity_clause(ctx *As_identity_clauseContext) {}

// ExitAs_identity_clause is called when production as_identity_clause is exited.
func (s *BaseDb2ParserListener) ExitAs_identity_clause(ctx *As_identity_clauseContext) {}

// EnterAs_identity_clause_opts is called when production as_identity_clause_opts is entered.
func (s *BaseDb2ParserListener) EnterAs_identity_clause_opts(ctx *As_identity_clause_optsContext) {}

// ExitAs_identity_clause_opts is called when production as_identity_clause_opts is exited.
func (s *BaseDb2ParserListener) ExitAs_identity_clause_opts(ctx *As_identity_clause_optsContext) {}

// EnterPeriod_definition_alter is called when production period_definition_alter is entered.
func (s *BaseDb2ParserListener) EnterPeriod_definition_alter(ctx *Period_definition_alterContext) {}

// ExitPeriod_definition_alter is called when production period_definition_alter is exited.
func (s *BaseDb2ParserListener) ExitPeriod_definition_alter(ctx *Period_definition_alterContext) {}

// EnterAdd_partition is called when production add_partition is entered.
func (s *BaseDb2ParserListener) EnterAdd_partition(ctx *Add_partitionContext) {}

// ExitAdd_partition is called when production add_partition is exited.
func (s *BaseDb2ParserListener) ExitAdd_partition(ctx *Add_partitionContext) {}

// EnterBoundary_spec_alter is called when production boundary_spec_alter is entered.
func (s *BaseDb2ParserListener) EnterBoundary_spec_alter(ctx *Boundary_spec_alterContext) {}

// ExitBoundary_spec_alter is called when production boundary_spec_alter is exited.
func (s *BaseDb2ParserListener) ExitBoundary_spec_alter(ctx *Boundary_spec_alterContext) {}

// EnterAttach_partition is called when production attach_partition is entered.
func (s *BaseDb2ParserListener) EnterAttach_partition(ctx *Attach_partitionContext) {}

// ExitAttach_partition is called when production attach_partition is exited.
func (s *BaseDb2ParserListener) ExitAttach_partition(ctx *Attach_partitionContext) {}

// EnterActivate_deactivate is called when production activate_deactivate is entered.
func (s *BaseDb2ParserListener) EnterActivate_deactivate(ctx *Activate_deactivateContext) {}

// ExitActivate_deactivate is called when production activate_deactivate is exited.
func (s *BaseDb2ParserListener) ExitActivate_deactivate(ctx *Activate_deactivateContext) {}

// EnterAlter_tablespace_statement is called when production alter_tablespace_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_tablespace_statement(ctx *Alter_tablespace_statementContext) {
}

// ExitAlter_tablespace_statement is called when production alter_tablespace_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_tablespace_statement(ctx *Alter_tablespace_statementContext) {
}

// EnterAlter_tablespace_opts is called when production alter_tablespace_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_tablespace_opts(ctx *Alter_tablespace_optsContext) {}

// ExitAlter_tablespace_opts is called when production alter_tablespace_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_tablespace_opts(ctx *Alter_tablespace_optsContext) {}

// EnterAdd_clause is called when production add_clause is entered.
func (s *BaseDb2ParserListener) EnterAdd_clause(ctx *Add_clauseContext) {}

// ExitAdd_clause is called when production add_clause is exited.
func (s *BaseDb2ParserListener) ExitAdd_clause(ctx *Add_clauseContext) {}

// EnterDb_container_clause is called when production db_container_clause is entered.
func (s *BaseDb2ParserListener) EnterDb_container_clause(ctx *Db_container_clauseContext) {}

// ExitDb_container_clause is called when production db_container_clause is exited.
func (s *BaseDb2ParserListener) ExitDb_container_clause(ctx *Db_container_clauseContext) {}

// EnterDb_container_clause_opts is called when production db_container_clause_opts is entered.
func (s *BaseDb2ParserListener) EnterDb_container_clause_opts(ctx *Db_container_clause_optsContext) {}

// ExitDb_container_clause_opts is called when production db_container_clause_opts is exited.
func (s *BaseDb2ParserListener) ExitDb_container_clause_opts(ctx *Db_container_clause_optsContext) {}

// EnterDrop_container_clause is called when production drop_container_clause is entered.
func (s *BaseDb2ParserListener) EnterDrop_container_clause(ctx *Drop_container_clauseContext) {}

// ExitDrop_container_clause is called when production drop_container_clause is exited.
func (s *BaseDb2ParserListener) ExitDrop_container_clause(ctx *Drop_container_clauseContext) {}

// EnterFile_device is called when production file_device is entered.
func (s *BaseDb2ParserListener) EnterFile_device(ctx *File_deviceContext) {}

// ExitFile_device is called when production file_device is exited.
func (s *BaseDb2ParserListener) ExitFile_device(ctx *File_deviceContext) {}

// EnterAll_containers_clause is called when production all_containers_clause is entered.
func (s *BaseDb2ParserListener) EnterAll_containers_clause(ctx *All_containers_clauseContext) {}

// ExitAll_containers_clause is called when production all_containers_clause is exited.
func (s *BaseDb2ParserListener) ExitAll_containers_clause(ctx *All_containers_clauseContext) {}

// EnterSystem_container_clause is called when production system_container_clause is entered.
func (s *BaseDb2ParserListener) EnterSystem_container_clause(ctx *System_container_clauseContext) {}

// ExitSystem_container_clause is called when production system_container_clause is exited.
func (s *BaseDb2ParserListener) ExitSystem_container_clause(ctx *System_container_clauseContext) {}

// EnterStripeset is called when production stripeset is entered.
func (s *BaseDb2ParserListener) EnterStripeset(ctx *StripesetContext) {}

// ExitStripeset is called when production stripeset is exited.
func (s *BaseDb2ParserListener) ExitStripeset(ctx *StripesetContext) {}

// EnterKm is called when production km is entered.
func (s *BaseDb2ParserListener) EnterKm(ctx *KmContext) {}

// ExitKm is called when production km is exited.
func (s *BaseDb2ParserListener) ExitKm(ctx *KmContext) {}

// EnterKmg_percent is called when production kmg_percent is entered.
func (s *BaseDb2ParserListener) EnterKmg_percent(ctx *Kmg_percentContext) {}

// ExitKmg_percent is called when production kmg_percent is exited.
func (s *BaseDb2ParserListener) ExitKmg_percent(ctx *Kmg_percentContext) {}

// EnterAlter_threshold_statement is called when production alter_threshold_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_threshold_statement(ctx *Alter_threshold_statementContext) {
}

// ExitAlter_threshold_statement is called when production alter_threshold_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_threshold_statement(ctx *Alter_threshold_statementContext) {
}

// EnterAlter_threshold_opts is called when production alter_threshold_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_threshold_opts(ctx *Alter_threshold_optsContext) {}

// ExitAlter_threshold_opts is called when production alter_threshold_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_threshold_opts(ctx *Alter_threshold_optsContext) {}

// EnterAlter_threshold_predicate is called when production alter_threshold_predicate is entered.
func (s *BaseDb2ParserListener) EnterAlter_threshold_predicate(ctx *Alter_threshold_predicateContext) {
}

// ExitAlter_threshold_predicate is called when production alter_threshold_predicate is exited.
func (s *BaseDb2ParserListener) ExitAlter_threshold_predicate(ctx *Alter_threshold_predicateContext) {
}

// EnterAlter_threshold_exceeded_actions is called when production alter_threshold_exceeded_actions is entered.
func (s *BaseDb2ParserListener) EnterAlter_threshold_exceeded_actions(ctx *Alter_threshold_exceeded_actionsContext) {
}

// ExitAlter_threshold_exceeded_actions is called when production alter_threshold_exceeded_actions is exited.
func (s *BaseDb2ParserListener) ExitAlter_threshold_exceeded_actions(ctx *Alter_threshold_exceeded_actionsContext) {
}

// EnterDt_units is called when production dt_units is entered.
func (s *BaseDb2ParserListener) EnterDt_units(ctx *Dt_unitsContext) {}

// ExitDt_units is called when production dt_units is exited.
func (s *BaseDb2ParserListener) ExitDt_units(ctx *Dt_unitsContext) {}

// EnterDt_units_with_seconds is called when production dt_units_with_seconds is entered.
func (s *BaseDb2ParserListener) EnterDt_units_with_seconds(ctx *Dt_units_with_secondsContext) {}

// ExitDt_units_with_seconds is called when production dt_units_with_seconds is exited.
func (s *BaseDb2ParserListener) ExitDt_units_with_seconds(ctx *Dt_units_with_secondsContext) {}

// EnterAlter_trigger_statement is called when production alter_trigger_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_trigger_statement(ctx *Alter_trigger_statementContext) {}

// ExitAlter_trigger_statement is called when production alter_trigger_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_trigger_statement(ctx *Alter_trigger_statementContext) {}

// EnterAlter_trusted_context_statement is called when production alter_trusted_context_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_trusted_context_statement(ctx *Alter_trusted_context_statementContext) {
}

// ExitAlter_trusted_context_statement is called when production alter_trusted_context_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_trusted_context_statement(ctx *Alter_trusted_context_statementContext) {
}

// EnterAlter_trusted_context_opts is called when production alter_trusted_context_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_trusted_context_opts(ctx *Alter_trusted_context_optsContext) {
}

// ExitAlter_trusted_context_opts is called when production alter_trusted_context_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_trusted_context_opts(ctx *Alter_trusted_context_optsContext) {
}

// EnterAlter_trusted_context_opts_alter_opts is called when production alter_trusted_context_opts_alter_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_trusted_context_opts_alter_opts(ctx *Alter_trusted_context_opts_alter_optsContext) {
}

// ExitAlter_trusted_context_opts_alter_opts is called when production alter_trusted_context_opts_alter_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_trusted_context_opts_alter_opts(ctx *Alter_trusted_context_opts_alter_optsContext) {
}

// EnterAddr_clause_encryption_val is called when production addr_clause_encryption_val is entered.
func (s *BaseDb2ParserListener) EnterAddr_clause_encryption_val(ctx *Addr_clause_encryption_valContext) {
}

// ExitAddr_clause_encryption_val is called when production addr_clause_encryption_val is exited.
func (s *BaseDb2ParserListener) ExitAddr_clause_encryption_val(ctx *Addr_clause_encryption_valContext) {
}

// EnterAddress_clause is called when production address_clause is entered.
func (s *BaseDb2ParserListener) EnterAddress_clause(ctx *Address_clauseContext) {}

// ExitAddress_clause is called when production address_clause is exited.
func (s *BaseDb2ParserListener) ExitAddress_clause(ctx *Address_clauseContext) {}

// EnterUser_clause is called when production user_clause is entered.
func (s *BaseDb2ParserListener) EnterUser_clause(ctx *User_clauseContext) {}

// ExitUser_clause is called when production user_clause is exited.
func (s *BaseDb2ParserListener) ExitUser_clause(ctx *User_clauseContext) {}

// EnterUse_for_opts is called when production use_for_opts is entered.
func (s *BaseDb2ParserListener) EnterUse_for_opts(ctx *Use_for_optsContext) {}

// ExitUse_for_opts is called when production use_for_opts is exited.
func (s *BaseDb2ParserListener) ExitUse_for_opts(ctx *Use_for_optsContext) {}

// EnterUse_for_opts_2 is called when production use_for_opts_2 is entered.
func (s *BaseDb2ParserListener) EnterUse_for_opts_2(ctx *Use_for_opts_2Context) {}

// ExitUse_for_opts_2 is called when production use_for_opts_2 is exited.
func (s *BaseDb2ParserListener) ExitUse_for_opts_2(ctx *Use_for_opts_2Context) {}

// EnterAlter_type_statement is called when production alter_type_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_type_statement(ctx *Alter_type_statementContext) {}

// ExitAlter_type_statement is called when production alter_type_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_type_statement(ctx *Alter_type_statementContext) {}

// EnterAlter_type_opts is called when production alter_type_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_type_opts(ctx *Alter_type_optsContext) {}

// ExitAlter_type_opts is called when production alter_type_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_type_opts(ctx *Alter_type_optsContext) {}

// EnterMethod_identifier is called when production method_identifier is entered.
func (s *BaseDb2ParserListener) EnterMethod_identifier(ctx *Method_identifierContext) {}

// ExitMethod_identifier is called when production method_identifier is exited.
func (s *BaseDb2ParserListener) ExitMethod_identifier(ctx *Method_identifierContext) {}

// EnterMethod_options is called when production method_options is entered.
func (s *BaseDb2ParserListener) EnterMethod_options(ctx *Method_optionsContext) {}

// ExitMethod_options is called when production method_options is exited.
func (s *BaseDb2ParserListener) ExitMethod_options(ctx *Method_optionsContext) {}

// EnterAlter_usage_list_statement is called when production alter_usage_list_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_usage_list_statement(ctx *Alter_usage_list_statementContext) {
}

// ExitAlter_usage_list_statement is called when production alter_usage_list_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_usage_list_statement(ctx *Alter_usage_list_statementContext) {
}

// EnterAlter_usage_list_opts_item is called when production alter_usage_list_opts_item is entered.
func (s *BaseDb2ParserListener) EnterAlter_usage_list_opts_item(ctx *Alter_usage_list_opts_itemContext) {
}

// ExitAlter_usage_list_opts_item is called when production alter_usage_list_opts_item is exited.
func (s *BaseDb2ParserListener) ExitAlter_usage_list_opts_item(ctx *Alter_usage_list_opts_itemContext) {
}

// EnterAlter_user_mapping_statement is called when production alter_user_mapping_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_user_mapping_statement(ctx *Alter_user_mapping_statementContext) {
}

// ExitAlter_user_mapping_statement is called when production alter_user_mapping_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_user_mapping_statement(ctx *Alter_user_mapping_statementContext) {
}

// EnterAlter_user_mapping_opts_item is called when production alter_user_mapping_opts_item is entered.
func (s *BaseDb2ParserListener) EnterAlter_user_mapping_opts_item(ctx *Alter_user_mapping_opts_itemContext) {
}

// ExitAlter_user_mapping_opts_item is called when production alter_user_mapping_opts_item is exited.
func (s *BaseDb2ParserListener) ExitAlter_user_mapping_opts_item(ctx *Alter_user_mapping_opts_itemContext) {
}

// EnterAdd_set is called when production add_set is entered.
func (s *BaseDb2ParserListener) EnterAdd_set(ctx *Add_setContext) {}

// ExitAdd_set is called when production add_set is exited.
func (s *BaseDb2ParserListener) ExitAdd_set(ctx *Add_setContext) {}

// EnterAlter_view_statement is called when production alter_view_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_view_statement(ctx *Alter_view_statementContext) {}

// ExitAlter_view_statement is called when production alter_view_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_view_statement(ctx *Alter_view_statementContext) {}

// EnterAlter_view_opts is called when production alter_view_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_view_opts(ctx *Alter_view_optsContext) {}

// ExitAlter_view_opts is called when production alter_view_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_view_opts(ctx *Alter_view_optsContext) {}

// EnterAlter_work_action_set_statement is called when production alter_work_action_set_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_work_action_set_statement(ctx *Alter_work_action_set_statementContext) {
}

// ExitAlter_work_action_set_statement is called when production alter_work_action_set_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_work_action_set_statement(ctx *Alter_work_action_set_statementContext) {
}

// EnterAlter_work_action_set_opts is called when production alter_work_action_set_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_work_action_set_opts(ctx *Alter_work_action_set_optsContext) {
}

// ExitAlter_work_action_set_opts is called when production alter_work_action_set_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_work_action_set_opts(ctx *Alter_work_action_set_optsContext) {
}

// EnterWork_action_alteration is called when production work_action_alteration is entered.
func (s *BaseDb2ParserListener) EnterWork_action_alteration(ctx *Work_action_alterationContext) {}

// ExitWork_action_alteration is called when production work_action_alteration is exited.
func (s *BaseDb2ParserListener) ExitWork_action_alteration(ctx *Work_action_alterationContext) {}

// EnterWork_action_alteration_opts is called when production work_action_alteration_opts is entered.
func (s *BaseDb2ParserListener) EnterWork_action_alteration_opts(ctx *Work_action_alteration_optsContext) {
}

// ExitWork_action_alteration_opts is called when production work_action_alteration_opts is exited.
func (s *BaseDb2ParserListener) ExitWork_action_alteration_opts(ctx *Work_action_alteration_optsContext) {
}

// EnterAlter_action_types_clause is called when production alter_action_types_clause is entered.
func (s *BaseDb2ParserListener) EnterAlter_action_types_clause(ctx *Alter_action_types_clauseContext) {
}

// ExitAlter_action_types_clause is called when production alter_action_types_clause is exited.
func (s *BaseDb2ParserListener) ExitAlter_action_types_clause(ctx *Alter_action_types_clauseContext) {
}

// EnterThreshold_predicate_clause is called when production threshold_predicate_clause is entered.
func (s *BaseDb2ParserListener) EnterThreshold_predicate_clause(ctx *Threshold_predicate_clauseContext) {
}

// ExitThreshold_predicate_clause is called when production threshold_predicate_clause is exited.
func (s *BaseDb2ParserListener) ExitThreshold_predicate_clause(ctx *Threshold_predicate_clauseContext) {
}

// EnterAlter_work_class_set_statement is called when production alter_work_class_set_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_work_class_set_statement(ctx *Alter_work_class_set_statementContext) {
}

// ExitAlter_work_class_set_statement is called when production alter_work_class_set_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_work_class_set_statement(ctx *Alter_work_class_set_statementContext) {
}

// EnterAlter_work_class_set_opts is called when production alter_work_class_set_opts is entered.
func (s *BaseDb2ParserListener) EnterAlter_work_class_set_opts(ctx *Alter_work_class_set_optsContext) {
}

// ExitAlter_work_class_set_opts is called when production alter_work_class_set_opts is exited.
func (s *BaseDb2ParserListener) ExitAlter_work_class_set_opts(ctx *Alter_work_class_set_optsContext) {
}

// EnterWork_class_alteration is called when production work_class_alteration is entered.
func (s *BaseDb2ParserListener) EnterWork_class_alteration(ctx *Work_class_alterationContext) {}

// ExitWork_class_alteration is called when production work_class_alteration is exited.
func (s *BaseDb2ParserListener) ExitWork_class_alteration(ctx *Work_class_alterationContext) {}

// EnterWork_class_alteration_opts is called when production work_class_alteration_opts is entered.
func (s *BaseDb2ParserListener) EnterWork_class_alteration_opts(ctx *Work_class_alteration_optsContext) {
}

// ExitWork_class_alteration_opts is called when production work_class_alteration_opts is exited.
func (s *BaseDb2ParserListener) ExitWork_class_alteration_opts(ctx *Work_class_alteration_optsContext) {
}

// EnterFor_from_to_alter_clause is called when production for_from_to_alter_clause is entered.
func (s *BaseDb2ParserListener) EnterFor_from_to_alter_clause(ctx *For_from_to_alter_clauseContext) {}

// ExitFor_from_to_alter_clause is called when production for_from_to_alter_clause is exited.
func (s *BaseDb2ParserListener) ExitFor_from_to_alter_clause(ctx *For_from_to_alter_clauseContext) {}

// EnterSchema_alter_clause is called when production schema_alter_clause is entered.
func (s *BaseDb2ParserListener) EnterSchema_alter_clause(ctx *Schema_alter_clauseContext) {}

// ExitSchema_alter_clause is called when production schema_alter_clause is exited.
func (s *BaseDb2ParserListener) ExitSchema_alter_clause(ctx *Schema_alter_clauseContext) {}

// EnterData_tag_alter_clause is called when production data_tag_alter_clause is entered.
func (s *BaseDb2ParserListener) EnterData_tag_alter_clause(ctx *Data_tag_alter_clauseContext) {}

// ExitData_tag_alter_clause is called when production data_tag_alter_clause is exited.
func (s *BaseDb2ParserListener) ExitData_tag_alter_clause(ctx *Data_tag_alter_clauseContext) {}

// EnterAlter_workload_statement is called when production alter_workload_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_workload_statement(ctx *Alter_workload_statementContext) {}

// ExitAlter_workload_statement is called when production alter_workload_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_workload_statement(ctx *Alter_workload_statementContext) {}

// EnterAlter_workload_opts_item is called when production alter_workload_opts_item is entered.
func (s *BaseDb2ParserListener) EnterAlter_workload_opts_item(ctx *Alter_workload_opts_itemContext) {}

// ExitAlter_workload_opts_item is called when production alter_workload_opts_item is exited.
func (s *BaseDb2ParserListener) ExitAlter_workload_opts_item(ctx *Alter_workload_opts_itemContext) {}

// EnterPackage_executable is called when production package_executable is entered.
func (s *BaseDb2ParserListener) EnterPackage_executable(ctx *Package_executableContext) {}

// ExitPackage_executable is called when production package_executable is exited.
func (s *BaseDb2ParserListener) ExitPackage_executable(ctx *Package_executableContext) {}

// EnterBase_none is called when production base_none is entered.
func (s *BaseDb2ParserListener) EnterBase_none(ctx *Base_noneContext) {}

// ExitBase_none is called when production base_none is exited.
func (s *BaseDb2ParserListener) ExitBase_none(ctx *Base_noneContext) {}

// EnterExtended_base_none is called when production extended_base_none is entered.
func (s *BaseDb2ParserListener) EnterExtended_base_none(ctx *Extended_base_noneContext) {}

// ExitExtended_base_none is called when production extended_base_none is exited.
func (s *BaseDb2ParserListener) ExitExtended_base_none(ctx *Extended_base_noneContext) {}

// EnterAlter_collect_activity_data_clause is called when production alter_collect_activity_data_clause is entered.
func (s *BaseDb2ParserListener) EnterAlter_collect_activity_data_clause(ctx *Alter_collect_activity_data_clauseContext) {
}

// ExitAlter_collect_activity_data_clause is called when production alter_collect_activity_data_clause is exited.
func (s *BaseDb2ParserListener) ExitAlter_collect_activity_data_clause(ctx *Alter_collect_activity_data_clauseContext) {
}

// EnterWith_opts is called when production with_opts is entered.
func (s *BaseDb2ParserListener) EnterWith_opts(ctx *With_optsContext) {}

// ExitWith_opts is called when production with_opts is exited.
func (s *BaseDb2ParserListener) ExitWith_opts(ctx *With_optsContext) {}

// EnterAlter_collect_history_clause is called when production alter_collect_history_clause is entered.
func (s *BaseDb2ParserListener) EnterAlter_collect_history_clause(ctx *Alter_collect_history_clauseContext) {
}

// ExitAlter_collect_history_clause is called when production alter_collect_history_clause is exited.
func (s *BaseDb2ParserListener) ExitAlter_collect_history_clause(ctx *Alter_collect_history_clauseContext) {
}

// EnterAlter_collect_lock_wait_data_clause is called when production alter_collect_lock_wait_data_clause is entered.
func (s *BaseDb2ParserListener) EnterAlter_collect_lock_wait_data_clause(ctx *Alter_collect_lock_wait_data_clauseContext) {
}

// ExitAlter_collect_lock_wait_data_clause is called when production alter_collect_lock_wait_data_clause is exited.
func (s *BaseDb2ParserListener) ExitAlter_collect_lock_wait_data_clause(ctx *Alter_collect_lock_wait_data_clauseContext) {
}

// EnterAlter_wrapper_statement is called when production alter_wrapper_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_wrapper_statement(ctx *Alter_wrapper_statementContext) {}

// ExitAlter_wrapper_statement is called when production alter_wrapper_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_wrapper_statement(ctx *Alter_wrapper_statementContext) {}

// EnterAlter_wrapper_opts_item is called when production alter_wrapper_opts_item is entered.
func (s *BaseDb2ParserListener) EnterAlter_wrapper_opts_item(ctx *Alter_wrapper_opts_itemContext) {}

// ExitAlter_wrapper_opts_item is called when production alter_wrapper_opts_item is exited.
func (s *BaseDb2ParserListener) ExitAlter_wrapper_opts_item(ctx *Alter_wrapper_opts_itemContext) {}

// EnterAlter_xsrobject_statement is called when production alter_xsrobject_statement is entered.
func (s *BaseDb2ParserListener) EnterAlter_xsrobject_statement(ctx *Alter_xsrobject_statementContext) {
}

// ExitAlter_xsrobject_statement is called when production alter_xsrobject_statement is exited.
func (s *BaseDb2ParserListener) ExitAlter_xsrobject_statement(ctx *Alter_xsrobject_statementContext) {
}

// EnterString is called when production string is entered.
func (s *BaseDb2ParserListener) EnterString(ctx *StringContext) {}

// ExitString is called when production string is exited.
func (s *BaseDb2ParserListener) ExitString(ctx *StringContext) {}

// EnterString_constant is called when production string_constant is entered.
func (s *BaseDb2ParserListener) EnterString_constant(ctx *String_constantContext) {}

// ExitString_constant is called when production string_constant is exited.
func (s *BaseDb2ParserListener) ExitString_constant(ctx *String_constantContext) {}

// EnterNumeric_constant is called when production numeric_constant is entered.
func (s *BaseDb2ParserListener) EnterNumeric_constant(ctx *Numeric_constantContext) {}

// ExitNumeric_constant is called when production numeric_constant is exited.
func (s *BaseDb2ParserListener) ExitNumeric_constant(ctx *Numeric_constantContext) {}

// EnterData_type is called when production data_type is entered.
func (s *BaseDb2ParserListener) EnterData_type(ctx *Data_typeContext) {}

// ExitData_type is called when production data_type is exited.
func (s *BaseDb2ParserListener) ExitData_type(ctx *Data_typeContext) {}

// EnterAnchored_data_type is called when production anchored_data_type is entered.
func (s *BaseDb2ParserListener) EnterAnchored_data_type(ctx *Anchored_data_typeContext) {}

// ExitAnchored_data_type is called when production anchored_data_type is exited.
func (s *BaseDb2ParserListener) ExitAnchored_data_type(ctx *Anchored_data_typeContext) {}

// EnterAnchored_non_row_data_type is called when production anchored_non_row_data_type is entered.
func (s *BaseDb2ParserListener) EnterAnchored_non_row_data_type(ctx *Anchored_non_row_data_typeContext) {
}

// ExitAnchored_non_row_data_type is called when production anchored_non_row_data_type is exited.
func (s *BaseDb2ParserListener) ExitAnchored_non_row_data_type(ctx *Anchored_non_row_data_typeContext) {
}

// EnterAnchored_row_data_type is called when production anchored_row_data_type is entered.
func (s *BaseDb2ParserListener) EnterAnchored_row_data_type(ctx *Anchored_row_data_typeContext) {}

// ExitAnchored_row_data_type is called when production anchored_row_data_type is exited.
func (s *BaseDb2ParserListener) ExitAnchored_row_data_type(ctx *Anchored_row_data_typeContext) {}

// EnterSource_data_type is called when production source_data_type is entered.
func (s *BaseDb2ParserListener) EnterSource_data_type(ctx *Source_data_typeContext) {}

// ExitSource_data_type is called when production source_data_type is exited.
func (s *BaseDb2ParserListener) ExitSource_data_type(ctx *Source_data_typeContext) {}

// EnterData_type_constrainst is called when production data_type_constrainst is entered.
func (s *BaseDb2ParserListener) EnterData_type_constrainst(ctx *Data_type_constrainstContext) {}

// ExitData_type_constrainst is called when production data_type_constrainst is exited.
func (s *BaseDb2ParserListener) ExitData_type_constrainst(ctx *Data_type_constrainstContext) {}

// EnterCheck_condition is called when production check_condition is entered.
func (s *BaseDb2ParserListener) EnterCheck_condition(ctx *Check_conditionContext) {}

// ExitCheck_condition is called when production check_condition is exited.
func (s *BaseDb2ParserListener) ExitCheck_condition(ctx *Check_conditionContext) {}

// EnterData_type_2 is called when production data_type_2 is entered.
func (s *BaseDb2ParserListener) EnterData_type_2(ctx *Data_type_2Context) {}

// ExitData_type_2 is called when production data_type_2 is exited.
func (s *BaseDb2ParserListener) ExitData_type_2(ctx *Data_type_2Context) {}

// EnterBuilt_in_type is called when production built_in_type is entered.
func (s *BaseDb2ParserListener) EnterBuilt_in_type(ctx *Built_in_typeContext) {}

// ExitBuilt_in_type is called when production built_in_type is exited.
func (s *BaseDb2ParserListener) ExitBuilt_in_type(ctx *Built_in_typeContext) {}

// EnterInteger_paren is called when production integer_paren is entered.
func (s *BaseDb2ParserListener) EnterInteger_paren(ctx *Integer_parenContext) {}

// ExitInteger_paren is called when production integer_paren is exited.
func (s *BaseDb2ParserListener) ExitInteger_paren(ctx *Integer_parenContext) {}

// EnterInteger_kmg_paren is called when production integer_kmg_paren is entered.
func (s *BaseDb2ParserListener) EnterInteger_kmg_paren(ctx *Integer_kmg_parenContext) {}

// ExitInteger_kmg_paren is called when production integer_kmg_paren is exited.
func (s *BaseDb2ParserListener) ExitInteger_kmg_paren(ctx *Integer_kmg_parenContext) {}

// EnterChar_character is called when production char_character is entered.
func (s *BaseDb2ParserListener) EnterChar_character(ctx *Char_characterContext) {}

// ExitChar_character is called when production char_character is exited.
func (s *BaseDb2ParserListener) ExitChar_character(ctx *Char_characterContext) {}

// EnterOctets_codeunits is called when production octets_codeunits is entered.
func (s *BaseDb2ParserListener) EnterOctets_codeunits(ctx *Octets_codeunitsContext) {}

// ExitOctets_codeunits is called when production octets_codeunits is exited.
func (s *BaseDb2ParserListener) ExitOctets_codeunits(ctx *Octets_codeunitsContext) {}

// EnterCodeunits is called when production codeunits is entered.
func (s *BaseDb2ParserListener) EnterCodeunits(ctx *CodeunitsContext) {}

// ExitCodeunits is called when production codeunits is exited.
func (s *BaseDb2ParserListener) ExitCodeunits(ctx *CodeunitsContext) {}

// EnterKmg is called when production kmg is entered.
func (s *BaseDb2ParserListener) EnterKmg(ctx *KmgContext) {}

// ExitKmg is called when production kmg is exited.
func (s *BaseDb2ParserListener) ExitKmg(ctx *KmgContext) {}

// EnterRs_locator_variable is called when production rs_locator_variable is entered.
func (s *BaseDb2ParserListener) EnterRs_locator_variable(ctx *Rs_locator_variableContext) {}

// ExitRs_locator_variable is called when production rs_locator_variable is exited.
func (s *BaseDb2ParserListener) ExitRs_locator_variable(ctx *Rs_locator_variableContext) {}

// EnterInteger_constant_list is called when production integer_constant_list is entered.
func (s *BaseDb2ParserListener) EnterInteger_constant_list(ctx *Integer_constant_listContext) {}

// ExitInteger_constant_list is called when production integer_constant_list is exited.
func (s *BaseDb2ParserListener) ExitInteger_constant_list(ctx *Integer_constant_listContext) {}

// EnterInteger_constant is called when production integer_constant is entered.
func (s *BaseDb2ParserListener) EnterInteger_constant(ctx *Integer_constantContext) {}

// ExitInteger_constant is called when production integer_constant is exited.
func (s *BaseDb2ParserListener) ExitInteger_constant(ctx *Integer_constantContext) {}

// EnterInteger_value is called when production integer_value is entered.
func (s *BaseDb2ParserListener) EnterInteger_value(ctx *Integer_valueContext) {}

// ExitInteger_value is called when production integer_value is exited.
func (s *BaseDb2ParserListener) ExitInteger_value(ctx *Integer_valueContext) {}

// EnterPositive_integer is called when production positive_integer is entered.
func (s *BaseDb2ParserListener) EnterPositive_integer(ctx *Positive_integerContext) {}

// ExitPositive_integer is called when production positive_integer is exited.
func (s *BaseDb2ParserListener) ExitPositive_integer(ctx *Positive_integerContext) {}

// EnterBigint_value is called when production bigint_value is entered.
func (s *BaseDb2ParserListener) EnterBigint_value(ctx *Bigint_valueContext) {}

// ExitBigint_value is called when production bigint_value is exited.
func (s *BaseDb2ParserListener) ExitBigint_value(ctx *Bigint_valueContext) {}

// EnterBigint_constant is called when production bigint_constant is entered.
func (s *BaseDb2ParserListener) EnterBigint_constant(ctx *Bigint_constantContext) {}

// ExitBigint_constant is called when production bigint_constant is exited.
func (s *BaseDb2ParserListener) ExitBigint_constant(ctx *Bigint_constantContext) {}

// EnterMember_number is called when production member_number is entered.
func (s *BaseDb2ParserListener) EnterMember_number(ctx *Member_numberContext) {}

// ExitMember_number is called when production member_number is exited.
func (s *BaseDb2ParserListener) ExitMember_number(ctx *Member_numberContext) {}

// EnterVersion_id is called when production version_id is entered.
func (s *BaseDb2ParserListener) EnterVersion_id(ctx *Version_idContext) {}

// ExitVersion_id is called when production version_id is exited.
func (s *BaseDb2ParserListener) ExitVersion_id(ctx *Version_idContext) {}

// EnterDrop_statement is called when production drop_statement is entered.
func (s *BaseDb2ParserListener) EnterDrop_statement(ctx *Drop_statementContext) {}

// ExitDrop_statement is called when production drop_statement is exited.
func (s *BaseDb2ParserListener) ExitDrop_statement(ctx *Drop_statementContext) {}

// EnterAlias_designator is called when production alias_designator is entered.
func (s *BaseDb2ParserListener) EnterAlias_designator(ctx *Alias_designatorContext) {}

// ExitAlias_designator is called when production alias_designator is exited.
func (s *BaseDb2ParserListener) ExitAlias_designator(ctx *Alias_designatorContext) {}

// EnterService_class_designator is called when production service_class_designator is entered.
func (s *BaseDb2ParserListener) EnterService_class_designator(ctx *Service_class_designatorContext) {}

// ExitService_class_designator is called when production service_class_designator is exited.
func (s *BaseDb2ParserListener) ExitService_class_designator(ctx *Service_class_designatorContext) {}

// EnterTablespace_name_list is called when production tablespace_name_list is entered.
func (s *BaseDb2ParserListener) EnterTablespace_name_list(ctx *Tablespace_name_listContext) {}

// ExitTablespace_name_list is called when production tablespace_name_list is exited.
func (s *BaseDb2ParserListener) ExitTablespace_name_list(ctx *Tablespace_name_listContext) {}

// EnterAssociate_locators_statement is called when production associate_locators_statement is entered.
func (s *BaseDb2ParserListener) EnterAssociate_locators_statement(ctx *Associate_locators_statementContext) {
}

// ExitAssociate_locators_statement is called when production associate_locators_statement is exited.
func (s *BaseDb2ParserListener) ExitAssociate_locators_statement(ctx *Associate_locators_statementContext) {
}

// EnterAudit_statement is called when production audit_statement is entered.
func (s *BaseDb2ParserListener) EnterAudit_statement(ctx *Audit_statementContext) {}

// ExitAudit_statement is called when production audit_statement is exited.
func (s *BaseDb2ParserListener) ExitAudit_statement(ctx *Audit_statementContext) {}

// EnterBegin_declare_section_statement is called when production begin_declare_section_statement is entered.
func (s *BaseDb2ParserListener) EnterBegin_declare_section_statement(ctx *Begin_declare_section_statementContext) {
}

// ExitBegin_declare_section_statement is called when production begin_declare_section_statement is exited.
func (s *BaseDb2ParserListener) ExitBegin_declare_section_statement(ctx *Begin_declare_section_statementContext) {
}

// EnterCall_statement is called when production call_statement is entered.
func (s *BaseDb2ParserListener) EnterCall_statement(ctx *Call_statementContext) {}

// ExitCall_statement is called when production call_statement is exited.
func (s *BaseDb2ParserListener) ExitCall_statement(ctx *Call_statementContext) {}

// EnterArg_list_paren is called when production arg_list_paren is entered.
func (s *BaseDb2ParserListener) EnterArg_list_paren(ctx *Arg_list_parenContext) {}

// ExitArg_list_paren is called when production arg_list_paren is exited.
func (s *BaseDb2ParserListener) ExitArg_list_paren(ctx *Arg_list_parenContext) {}

// EnterArg_list is called when production arg_list is entered.
func (s *BaseDb2ParserListener) EnterArg_list(ctx *Arg_listContext) {}

// ExitArg_list is called when production arg_list is exited.
func (s *BaseDb2ParserListener) ExitArg_list(ctx *Arg_listContext) {}

// EnterArgument is called when production argument is entered.
func (s *BaseDb2ParserListener) EnterArgument(ctx *ArgumentContext) {}

// ExitArgument is called when production argument is exited.
func (s *BaseDb2ParserListener) ExitArgument(ctx *ArgumentContext) {}

// EnterCase_statement is called when production case_statement is entered.
func (s *BaseDb2ParserListener) EnterCase_statement(ctx *Case_statementContext) {}

// ExitCase_statement is called when production case_statement is exited.
func (s *BaseDb2ParserListener) ExitCase_statement(ctx *Case_statementContext) {}

// EnterSearched_case_statement_when_clause is called when production searched_case_statement_when_clause is entered.
func (s *BaseDb2ParserListener) EnterSearched_case_statement_when_clause(ctx *Searched_case_statement_when_clauseContext) {
}

// ExitSearched_case_statement_when_clause is called when production searched_case_statement_when_clause is exited.
func (s *BaseDb2ParserListener) ExitSearched_case_statement_when_clause(ctx *Searched_case_statement_when_clauseContext) {
}

// EnterSimple_case_statement_when_clause is called when production simple_case_statement_when_clause is entered.
func (s *BaseDb2ParserListener) EnterSimple_case_statement_when_clause(ctx *Simple_case_statement_when_clauseContext) {
}

// ExitSimple_case_statement_when_clause is called when production simple_case_statement_when_clause is exited.
func (s *BaseDb2ParserListener) ExitSimple_case_statement_when_clause(ctx *Simple_case_statement_when_clauseContext) {
}

// EnterClose_statement is called when production close_statement is entered.
func (s *BaseDb2ParserListener) EnterClose_statement(ctx *Close_statementContext) {}

// ExitClose_statement is called when production close_statement is exited.
func (s *BaseDb2ParserListener) ExitClose_statement(ctx *Close_statementContext) {}

// EnterComment_statement is called when production comment_statement is entered.
func (s *BaseDb2ParserListener) EnterComment_statement(ctx *Comment_statementContext) {}

// ExitComment_statement is called when production comment_statement is exited.
func (s *BaseDb2ParserListener) ExitComment_statement(ctx *Comment_statementContext) {}

// EnterColumn_comment is called when production column_comment is entered.
func (s *BaseDb2ParserListener) EnterColumn_comment(ctx *Column_commentContext) {}

// ExitColumn_comment is called when production column_comment is exited.
func (s *BaseDb2ParserListener) ExitColumn_comment(ctx *Column_commentContext) {}

// EnterComment_objects is called when production comment_objects is entered.
func (s *BaseDb2ParserListener) EnterComment_objects(ctx *Comment_objectsContext) {}

// ExitComment_objects is called when production comment_objects is exited.
func (s *BaseDb2ParserListener) ExitComment_objects(ctx *Comment_objectsContext) {}

// EnterCommit_statement is called when production commit_statement is entered.
func (s *BaseDb2ParserListener) EnterCommit_statement(ctx *Commit_statementContext) {}

// ExitCommit_statement is called when production commit_statement is exited.
func (s *BaseDb2ParserListener) ExitCommit_statement(ctx *Commit_statementContext) {}

// EnterConnect_type_1_statement is called when production connect_type_1_statement is entered.
func (s *BaseDb2ParserListener) EnterConnect_type_1_statement(ctx *Connect_type_1_statementContext) {}

// ExitConnect_type_1_statement is called when production connect_type_1_statement is exited.
func (s *BaseDb2ParserListener) ExitConnect_type_1_statement(ctx *Connect_type_1_statementContext) {}

// EnterAuthorization is called when production authorization is entered.
func (s *BaseDb2ParserListener) EnterAuthorization(ctx *AuthorizationContext) {}

// ExitAuthorization is called when production authorization is exited.
func (s *BaseDb2ParserListener) ExitAuthorization(ctx *AuthorizationContext) {}

// EnterPasswords is called when production passwords is entered.
func (s *BaseDb2ParserListener) EnterPasswords(ctx *PasswordsContext) {}

// ExitPasswords is called when production passwords is exited.
func (s *BaseDb2ParserListener) ExitPasswords(ctx *PasswordsContext) {}

// EnterLock_block is called when production lock_block is entered.
func (s *BaseDb2ParserListener) EnterLock_block(ctx *Lock_blockContext) {}

// ExitLock_block is called when production lock_block is exited.
func (s *BaseDb2ParserListener) ExitLock_block(ctx *Lock_blockContext) {}

// EnterAccesstoken is called when production accesstoken is entered.
func (s *BaseDb2ParserListener) EnterAccesstoken(ctx *AccesstokenContext) {}

// ExitAccesstoken is called when production accesstoken is exited.
func (s *BaseDb2ParserListener) ExitAccesstoken(ctx *AccesstokenContext) {}

// EnterToken is called when production token is entered.
func (s *BaseDb2ParserListener) EnterToken(ctx *TokenContext) {}

// ExitToken is called when production token is exited.
func (s *BaseDb2ParserListener) ExitToken(ctx *TokenContext) {}

// EnterApi_key is called when production api_key is entered.
func (s *BaseDb2ParserListener) EnterApi_key(ctx *Api_keyContext) {}

// ExitApi_key is called when production api_key is exited.
func (s *BaseDb2ParserListener) ExitApi_key(ctx *Api_keyContext) {}

// EnterToken_type is called when production token_type is entered.
func (s *BaseDb2ParserListener) EnterToken_type(ctx *Token_typeContext) {}

// ExitToken_type is called when production token_type is exited.
func (s *BaseDb2ParserListener) ExitToken_type(ctx *Token_typeContext) {}

// EnterDeclare_cursor_statement is called when production declare_cursor_statement is entered.
func (s *BaseDb2ParserListener) EnterDeclare_cursor_statement(ctx *Declare_cursor_statementContext) {}

// ExitDeclare_cursor_statement is called when production declare_cursor_statement is exited.
func (s *BaseDb2ParserListener) ExitDeclare_cursor_statement(ctx *Declare_cursor_statementContext) {}

// EnterDeclare_global_temporary_table_statement is called when production declare_global_temporary_table_statement is entered.
func (s *BaseDb2ParserListener) EnterDeclare_global_temporary_table_statement(ctx *Declare_global_temporary_table_statementContext) {
}

// ExitDeclare_global_temporary_table_statement is called when production declare_global_temporary_table_statement is exited.
func (s *BaseDb2ParserListener) ExitDeclare_global_temporary_table_statement(ctx *Declare_global_temporary_table_statementContext) {
}

// EnterDescribe_statement is called when production describe_statement is entered.
func (s *BaseDb2ParserListener) EnterDescribe_statement(ctx *Describe_statementContext) {}

// ExitDescribe_statement is called when production describe_statement is exited.
func (s *BaseDb2ParserListener) ExitDescribe_statement(ctx *Describe_statementContext) {}

// EnterXquery_statement is called when production xquery_statement is entered.
func (s *BaseDb2ParserListener) EnterXquery_statement(ctx *Xquery_statementContext) {}

// ExitXquery_statement is called when production xquery_statement is exited.
func (s *BaseDb2ParserListener) ExitXquery_statement(ctx *Xquery_statementContext) {}

// EnterDescribe_input_statement is called when production describe_input_statement is entered.
func (s *BaseDb2ParserListener) EnterDescribe_input_statement(ctx *Describe_input_statementContext) {}

// ExitDescribe_input_statement is called when production describe_input_statement is exited.
func (s *BaseDb2ParserListener) ExitDescribe_input_statement(ctx *Describe_input_statementContext) {}

// EnterDescribe_output_statement is called when production describe_output_statement is entered.
func (s *BaseDb2ParserListener) EnterDescribe_output_statement(ctx *Describe_output_statementContext) {
}

// ExitDescribe_output_statement is called when production describe_output_statement is exited.
func (s *BaseDb2ParserListener) ExitDescribe_output_statement(ctx *Describe_output_statementContext) {
}

// EnterDisconnect_statement is called when production disconnect_statement is entered.
func (s *BaseDb2ParserListener) EnterDisconnect_statement(ctx *Disconnect_statementContext) {}

// ExitDisconnect_statement is called when production disconnect_statement is exited.
func (s *BaseDb2ParserListener) ExitDisconnect_statement(ctx *Disconnect_statementContext) {}

// EnterEnd_declare_section_statement is called when production end_declare_section_statement is entered.
func (s *BaseDb2ParserListener) EnterEnd_declare_section_statement(ctx *End_declare_section_statementContext) {
}

// ExitEnd_declare_section_statement is called when production end_declare_section_statement is exited.
func (s *BaseDb2ParserListener) ExitEnd_declare_section_statement(ctx *End_declare_section_statementContext) {
}

// EnterExecute_statement is called when production execute_statement is entered.
func (s *BaseDb2ParserListener) EnterExecute_statement(ctx *Execute_statementContext) {}

// ExitExecute_statement is called when production execute_statement is exited.
func (s *BaseDb2ParserListener) ExitExecute_statement(ctx *Execute_statementContext) {}

// EnterHost_variable_expression is called when production host_variable_expression is entered.
func (s *BaseDb2ParserListener) EnterHost_variable_expression(ctx *Host_variable_expressionContext) {}

// ExitHost_variable_expression is called when production host_variable_expression is exited.
func (s *BaseDb2ParserListener) ExitHost_variable_expression(ctx *Host_variable_expressionContext) {}

// EnterAssignment_target is called when production assignment_target is entered.
func (s *BaseDb2ParserListener) EnterAssignment_target(ctx *Assignment_targetContext) {}

// ExitAssignment_target is called when production assignment_target is exited.
func (s *BaseDb2ParserListener) ExitAssignment_target(ctx *Assignment_targetContext) {}

// EnterExecute_immediate_statement is called when production execute_immediate_statement is entered.
func (s *BaseDb2ParserListener) EnterExecute_immediate_statement(ctx *Execute_immediate_statementContext) {
}

// ExitExecute_immediate_statement is called when production execute_immediate_statement is exited.
func (s *BaseDb2ParserListener) ExitExecute_immediate_statement(ctx *Execute_immediate_statementContext) {
}

// EnterExplain_statement is called when production explain_statement is entered.
func (s *BaseDb2ParserListener) EnterExplain_statement(ctx *Explain_statementContext) {}

// ExitExplain_statement is called when production explain_statement is exited.
func (s *BaseDb2ParserListener) ExitExplain_statement(ctx *Explain_statementContext) {}

// EnterExplainable_sql_statement is called when production explainable_sql_statement is entered.
func (s *BaseDb2ParserListener) EnterExplainable_sql_statement(ctx *Explainable_sql_statementContext) {
}

// ExitExplainable_sql_statement is called when production explainable_sql_statement is exited.
func (s *BaseDb2ParserListener) ExitExplainable_sql_statement(ctx *Explainable_sql_statementContext) {
}

// EnterFetch_statement is called when production fetch_statement is entered.
func (s *BaseDb2ParserListener) EnterFetch_statement(ctx *Fetch_statementContext) {}

// ExitFetch_statement is called when production fetch_statement is exited.
func (s *BaseDb2ParserListener) ExitFetch_statement(ctx *Fetch_statementContext) {}

// EnterFlush_bufferpools_statement is called when production flush_bufferpools_statement is entered.
func (s *BaseDb2ParserListener) EnterFlush_bufferpools_statement(ctx *Flush_bufferpools_statementContext) {
}

// ExitFlush_bufferpools_statement is called when production flush_bufferpools_statement is exited.
func (s *BaseDb2ParserListener) ExitFlush_bufferpools_statement(ctx *Flush_bufferpools_statementContext) {
}

// EnterFlush_event_monitor_statement is called when production flush_event_monitor_statement is entered.
func (s *BaseDb2ParserListener) EnterFlush_event_monitor_statement(ctx *Flush_event_monitor_statementContext) {
}

// ExitFlush_event_monitor_statement is called when production flush_event_monitor_statement is exited.
func (s *BaseDb2ParserListener) ExitFlush_event_monitor_statement(ctx *Flush_event_monitor_statementContext) {
}

// EnterFlush_federated_cache_statement is called when production flush_federated_cache_statement is entered.
func (s *BaseDb2ParserListener) EnterFlush_federated_cache_statement(ctx *Flush_federated_cache_statementContext) {
}

// ExitFlush_federated_cache_statement is called when production flush_federated_cache_statement is exited.
func (s *BaseDb2ParserListener) ExitFlush_federated_cache_statement(ctx *Flush_federated_cache_statementContext) {
}

// EnterFlush_optimization_profile_cache_statement is called when production flush_optimization_profile_cache_statement is entered.
func (s *BaseDb2ParserListener) EnterFlush_optimization_profile_cache_statement(ctx *Flush_optimization_profile_cache_statementContext) {
}

// ExitFlush_optimization_profile_cache_statement is called when production flush_optimization_profile_cache_statement is exited.
func (s *BaseDb2ParserListener) ExitFlush_optimization_profile_cache_statement(ctx *Flush_optimization_profile_cache_statementContext) {
}

// EnterFlush_package_cache_statement is called when production flush_package_cache_statement is entered.
func (s *BaseDb2ParserListener) EnterFlush_package_cache_statement(ctx *Flush_package_cache_statementContext) {
}

// ExitFlush_package_cache_statement is called when production flush_package_cache_statement is exited.
func (s *BaseDb2ParserListener) ExitFlush_package_cache_statement(ctx *Flush_package_cache_statementContext) {
}

// EnterFlush_authentication_cache_statement is called when production flush_authentication_cache_statement is entered.
func (s *BaseDb2ParserListener) EnterFlush_authentication_cache_statement(ctx *Flush_authentication_cache_statementContext) {
}

// ExitFlush_authentication_cache_statement is called when production flush_authentication_cache_statement is exited.
func (s *BaseDb2ParserListener) ExitFlush_authentication_cache_statement(ctx *Flush_authentication_cache_statementContext) {
}

// EnterFree_locator_statement is called when production free_locator_statement is entered.
func (s *BaseDb2ParserListener) EnterFree_locator_statement(ctx *Free_locator_statementContext) {}

// ExitFree_locator_statement is called when production free_locator_statement is exited.
func (s *BaseDb2ParserListener) ExitFree_locator_statement(ctx *Free_locator_statementContext) {}

// EnterGet_diagnostics_statement is called when production get_diagnostics_statement is entered.
func (s *BaseDb2ParserListener) EnterGet_diagnostics_statement(ctx *Get_diagnostics_statementContext) {
}

// ExitGet_diagnostics_statement is called when production get_diagnostics_statement is exited.
func (s *BaseDb2ParserListener) ExitGet_diagnostics_statement(ctx *Get_diagnostics_statementContext) {
}

// EnterStatement_information is called when production statement_information is entered.
func (s *BaseDb2ParserListener) EnterStatement_information(ctx *Statement_informationContext) {}

// ExitStatement_information is called when production statement_information is exited.
func (s *BaseDb2ParserListener) ExitStatement_information(ctx *Statement_informationContext) {}

// EnterCondition_information is called when production condition_information is entered.
func (s *BaseDb2ParserListener) EnterCondition_information(ctx *Condition_informationContext) {}

// ExitCondition_information is called when production condition_information is exited.
func (s *BaseDb2ParserListener) ExitCondition_information(ctx *Condition_informationContext) {}

// EnterCondition_var_assignment is called when production condition_var_assignment is entered.
func (s *BaseDb2ParserListener) EnterCondition_var_assignment(ctx *Condition_var_assignmentContext) {}

// ExitCondition_var_assignment is called when production condition_var_assignment is exited.
func (s *BaseDb2ParserListener) ExitCondition_var_assignment(ctx *Condition_var_assignmentContext) {}

// EnterLock_table_statement is called when production lock_table_statement is entered.
func (s *BaseDb2ParserListener) EnterLock_table_statement(ctx *Lock_table_statementContext) {}

// ExitLock_table_statement is called when production lock_table_statement is exited.
func (s *BaseDb2ParserListener) ExitLock_table_statement(ctx *Lock_table_statementContext) {}

// EnterPipe_statement is called when production pipe_statement is entered.
func (s *BaseDb2ParserListener) EnterPipe_statement(ctx *Pipe_statementContext) {}

// ExitPipe_statement is called when production pipe_statement is exited.
func (s *BaseDb2ParserListener) ExitPipe_statement(ctx *Pipe_statementContext) {}

// EnterRefresh_table_statement is called when production refresh_table_statement is entered.
func (s *BaseDb2ParserListener) EnterRefresh_table_statement(ctx *Refresh_table_statementContext) {}

// ExitRefresh_table_statement is called when production refresh_table_statement is exited.
func (s *BaseDb2ParserListener) ExitRefresh_table_statement(ctx *Refresh_table_statementContext) {}

// EnterRelease_connection_statement is called when production release_connection_statement is entered.
func (s *BaseDb2ParserListener) EnterRelease_connection_statement(ctx *Release_connection_statementContext) {
}

// ExitRelease_connection_statement is called when production release_connection_statement is exited.
func (s *BaseDb2ParserListener) ExitRelease_connection_statement(ctx *Release_connection_statementContext) {
}

// EnterRename_statement is called when production rename_statement is entered.
func (s *BaseDb2ParserListener) EnterRename_statement(ctx *Rename_statementContext) {}

// ExitRename_statement is called when production rename_statement is exited.
func (s *BaseDb2ParserListener) ExitRename_statement(ctx *Rename_statementContext) {}

// EnterRename_stogroup_statement is called when production rename_stogroup_statement is entered.
func (s *BaseDb2ParserListener) EnterRename_stogroup_statement(ctx *Rename_stogroup_statementContext) {
}

// ExitRename_stogroup_statement is called when production rename_stogroup_statement is exited.
func (s *BaseDb2ParserListener) ExitRename_stogroup_statement(ctx *Rename_stogroup_statementContext) {
}

// EnterRename_tablespace_statement is called when production rename_tablespace_statement is entered.
func (s *BaseDb2ParserListener) EnterRename_tablespace_statement(ctx *Rename_tablespace_statementContext) {
}

// ExitRename_tablespace_statement is called when production rename_tablespace_statement is exited.
func (s *BaseDb2ParserListener) ExitRename_tablespace_statement(ctx *Rename_tablespace_statementContext) {
}

// EnterSet_statement is called when production set_statement is entered.
func (s *BaseDb2ParserListener) EnterSet_statement(ctx *Set_statementContext) {}

// ExitSet_statement is called when production set_statement is exited.
func (s *BaseDb2ParserListener) ExitSet_statement(ctx *Set_statementContext) {}

// EnterAccess_mode_clause is called when production access_mode_clause is entered.
func (s *BaseDb2ParserListener) EnterAccess_mode_clause(ctx *Access_mode_clauseContext) {}

// ExitAccess_mode_clause is called when production access_mode_clause is exited.
func (s *BaseDb2ParserListener) ExitAccess_mode_clause(ctx *Access_mode_clauseContext) {}

// EnterCascade_clause is called when production cascade_clause is entered.
func (s *BaseDb2ParserListener) EnterCascade_clause(ctx *Cascade_clauseContext) {}

// ExitCascade_clause is called when production cascade_clause is exited.
func (s *BaseDb2ParserListener) ExitCascade_clause(ctx *Cascade_clauseContext) {}

// EnterTo_descendent_types is called when production to_descendent_types is entered.
func (s *BaseDb2ParserListener) EnterTo_descendent_types(ctx *To_descendent_typesContext) {}

// ExitTo_descendent_types is called when production to_descendent_types is exited.
func (s *BaseDb2ParserListener) ExitTo_descendent_types(ctx *To_descendent_typesContext) {}

// EnterTable_type_list is called when production table_type_list is entered.
func (s *BaseDb2ParserListener) EnterTable_type_list(ctx *Table_type_listContext) {}

// ExitTable_type_list is called when production table_type_list is exited.
func (s *BaseDb2ParserListener) ExitTable_type_list(ctx *Table_type_listContext) {}

// EnterTable_type is called when production table_type is entered.
func (s *BaseDb2ParserListener) EnterTable_type(ctx *Table_typeContext) {}

// ExitTable_type is called when production table_type is exited.
func (s *BaseDb2ParserListener) ExitTable_type(ctx *Table_typeContext) {}

// EnterTable_checked_options_list is called when production table_checked_options_list is entered.
func (s *BaseDb2ParserListener) EnterTable_checked_options_list(ctx *Table_checked_options_listContext) {
}

// ExitTable_checked_options_list is called when production table_checked_options_list is exited.
func (s *BaseDb2ParserListener) ExitTable_checked_options_list(ctx *Table_checked_options_listContext) {
}

// EnterTable_checked_options is called when production table_checked_options is entered.
func (s *BaseDb2ParserListener) EnterTable_checked_options(ctx *Table_checked_optionsContext) {}

// ExitTable_checked_options is called when production table_checked_options is exited.
func (s *BaseDb2ParserListener) ExitTable_checked_options(ctx *Table_checked_optionsContext) {}

// EnterOnline_options is called when production online_options is entered.
func (s *BaseDb2ParserListener) EnterOnline_options(ctx *Online_optionsContext) {}

// ExitOnline_options is called when production online_options is exited.
func (s *BaseDb2ParserListener) ExitOnline_options(ctx *Online_optionsContext) {}

// EnterQuery_optimization_options is called when production query_optimization_options is entered.
func (s *BaseDb2ParserListener) EnterQuery_optimization_options(ctx *Query_optimization_optionsContext) {
}

// ExitQuery_optimization_options is called when production query_optimization_options is exited.
func (s *BaseDb2ParserListener) ExitQuery_optimization_options(ctx *Query_optimization_optionsContext) {
}

// EnterCheck_options is called when production check_options is entered.
func (s *BaseDb2ParserListener) EnterCheck_options(ctx *Check_optionsContext) {}

// ExitCheck_options is called when production check_options is exited.
func (s *BaseDb2ParserListener) ExitCheck_options(ctx *Check_optionsContext) {}

// EnterIncremental_options is called when production incremental_options is entered.
func (s *BaseDb2ParserListener) EnterIncremental_options(ctx *Incremental_optionsContext) {}

// ExitIncremental_options is called when production incremental_options is exited.
func (s *BaseDb2ParserListener) ExitIncremental_options(ctx *Incremental_optionsContext) {}

// EnterException_clause is called when production exception_clause is entered.
func (s *BaseDb2ParserListener) EnterException_clause(ctx *Exception_clauseContext) {}

// ExitException_clause is called when production exception_clause is exited.
func (s *BaseDb2ParserListener) ExitException_clause(ctx *Exception_clauseContext) {}

// EnterIn_table_use_clause is called when production in_table_use_clause is entered.
func (s *BaseDb2ParserListener) EnterIn_table_use_clause(ctx *In_table_use_clauseContext) {}

// ExitIn_table_use_clause is called when production in_table_use_clause is exited.
func (s *BaseDb2ParserListener) ExitIn_table_use_clause(ctx *In_table_use_clauseContext) {}

// EnterTable_unchecked_options is called when production table_unchecked_options is entered.
func (s *BaseDb2ParserListener) EnterTable_unchecked_options(ctx *Table_unchecked_optionsContext) {}

// ExitTable_unchecked_options is called when production table_unchecked_options is exited.
func (s *BaseDb2ParserListener) ExitTable_unchecked_options(ctx *Table_unchecked_optionsContext) {}

// EnterFull_access is called when production full_access is entered.
func (s *BaseDb2ParserListener) EnterFull_access(ctx *Full_accessContext) {}

// ExitFull_access is called when production full_access is exited.
func (s *BaseDb2ParserListener) ExitFull_access(ctx *Full_accessContext) {}

// EnterIntegrity_options is called when production integrity_options is entered.
func (s *BaseDb2ParserListener) EnterIntegrity_options(ctx *Integrity_optionsContext) {}

// ExitIntegrity_options is called when production integrity_options is exited.
func (s *BaseDb2ParserListener) ExitIntegrity_options(ctx *Integrity_optionsContext) {}

// EnterIntegrity_options_item is called when production integrity_options_item is entered.
func (s *BaseDb2ParserListener) EnterIntegrity_options_item(ctx *Integrity_options_itemContext) {}

// ExitIntegrity_options_item is called when production integrity_options_item is exited.
func (s *BaseDb2ParserListener) ExitIntegrity_options_item(ctx *Integrity_options_itemContext) {}

// EnterVar_def_list is called when production var_def_list is entered.
func (s *BaseDb2ParserListener) EnterVar_def_list(ctx *Var_def_listContext) {}

// ExitVar_def_list is called when production var_def_list is exited.
func (s *BaseDb2ParserListener) ExitVar_def_list(ctx *Var_def_listContext) {}

// EnterVar_def is called when production var_def is entered.
func (s *BaseDb2ParserListener) EnterVar_def(ctx *Var_defContext) {}

// ExitVar_def is called when production var_def is exited.
func (s *BaseDb2ParserListener) ExitVar_def(ctx *Var_defContext) {}

// EnterExpr_null is called when production expr_null is entered.
func (s *BaseDb2ParserListener) EnterExpr_null(ctx *Expr_nullContext) {}

// ExitExpr_null is called when production expr_null is exited.
func (s *BaseDb2ParserListener) ExitExpr_null(ctx *Expr_nullContext) {}

// EnterExpr_null_default is called when production expr_null_default is entered.
func (s *BaseDb2ParserListener) EnterExpr_null_default(ctx *Expr_null_defaultContext) {}

// ExitExpr_null_default is called when production expr_null_default is exited.
func (s *BaseDb2ParserListener) ExitExpr_null_default(ctx *Expr_null_defaultContext) {}

// EnterArray_index is called when production array_index is entered.
func (s *BaseDb2ParserListener) EnterArray_index(ctx *Array_indexContext) {}

// ExitArray_index is called when production array_index is exited.
func (s *BaseDb2ParserListener) ExitArray_index(ctx *Array_indexContext) {}

// EnterRow_fullselect is called when production row_fullselect is entered.
func (s *BaseDb2ParserListener) EnterRow_fullselect(ctx *Row_fullselectContext) {}

// ExitRow_fullselect is called when production row_fullselect is exited.
func (s *BaseDb2ParserListener) ExitRow_fullselect(ctx *Row_fullselectContext) {}

// EnterTarget_variable is called when production target_variable is entered.
func (s *BaseDb2ParserListener) EnterTarget_variable(ctx *Target_variableContext) {}

// ExitTarget_variable is called when production target_variable is exited.
func (s *BaseDb2ParserListener) ExitTarget_variable(ctx *Target_variableContext) {}

// EnterTarget_cursor_variable is called when production target_cursor_variable is entered.
func (s *BaseDb2ParserListener) EnterTarget_cursor_variable(ctx *Target_cursor_variableContext) {}

// ExitTarget_cursor_variable is called when production target_cursor_variable is exited.
func (s *BaseDb2ParserListener) ExitTarget_cursor_variable(ctx *Target_cursor_variableContext) {}

// EnterTarget_row_variable is called when production target_row_variable is entered.
func (s *BaseDb2ParserListener) EnterTarget_row_variable(ctx *Target_row_variableContext) {}

// ExitTarget_row_variable is called when production target_row_variable is exited.
func (s *BaseDb2ParserListener) ExitTarget_row_variable(ctx *Target_row_variableContext) {}

// EnterRow_array_element_specification is called when production row_array_element_specification is entered.
func (s *BaseDb2ParserListener) EnterRow_array_element_specification(ctx *Row_array_element_specificationContext) {
}

// ExitRow_array_element_specification is called when production row_array_element_specification is exited.
func (s *BaseDb2ParserListener) ExitRow_array_element_specification(ctx *Row_array_element_specificationContext) {
}

// EnterRow_field_reference is called when production row_field_reference is entered.
func (s *BaseDb2ParserListener) EnterRow_field_reference(ctx *Row_field_referenceContext) {}

// ExitRow_field_reference is called when production row_field_reference is exited.
func (s *BaseDb2ParserListener) ExitRow_field_reference(ctx *Row_field_referenceContext) {}

// EnterField_reference is called when production field_reference is entered.
func (s *BaseDb2ParserListener) EnterField_reference(ctx *Field_referenceContext) {}

// ExitField_reference is called when production field_reference is exited.
func (s *BaseDb2ParserListener) ExitField_reference(ctx *Field_referenceContext) {}

// EnterSearch_condition is called when production search_condition is entered.
func (s *BaseDb2ParserListener) EnterSearch_condition(ctx *Search_conditionContext) {}

// ExitSearch_condition is called when production search_condition is exited.
func (s *BaseDb2ParserListener) ExitSearch_condition(ctx *Search_conditionContext) {}

// EnterPredicate is called when production predicate is entered.
func (s *BaseDb2ParserListener) EnterPredicate(ctx *PredicateContext) {}

// ExitPredicate is called when production predicate is exited.
func (s *BaseDb2ParserListener) ExitPredicate(ctx *PredicateContext) {}

// EnterAccording_to_clause is called when production according_to_clause is entered.
func (s *BaseDb2ParserListener) EnterAccording_to_clause(ctx *According_to_clauseContext) {}

// ExitAccording_to_clause is called when production according_to_clause is exited.
func (s *BaseDb2ParserListener) ExitAccording_to_clause(ctx *According_to_clauseContext) {}

// EnterXml_schema_identification_list is called when production xml_schema_identification_list is entered.
func (s *BaseDb2ParserListener) EnterXml_schema_identification_list(ctx *Xml_schema_identification_listContext) {
}

// ExitXml_schema_identification_list is called when production xml_schema_identification_list is exited.
func (s *BaseDb2ParserListener) ExitXml_schema_identification_list(ctx *Xml_schema_identification_listContext) {
}

// EnterXml_schema_identification is called when production xml_schema_identification is entered.
func (s *BaseDb2ParserListener) EnterXml_schema_identification(ctx *Xml_schema_identificationContext) {
}

// ExitXml_schema_identification is called when production xml_schema_identification is exited.
func (s *BaseDb2ParserListener) ExitXml_schema_identification(ctx *Xml_schema_identificationContext) {
}

// EnterFullselect_in_parentheses is called when production fullselect_in_parentheses is entered.
func (s *BaseDb2ParserListener) EnterFullselect_in_parentheses(ctx *Fullselect_in_parenthesesContext) {
}

// ExitFullselect_in_parentheses is called when production fullselect_in_parentheses is exited.
func (s *BaseDb2ParserListener) ExitFullselect_in_parentheses(ctx *Fullselect_in_parenthesesContext) {
}

// EnterSome_any_all is called when production some_any_all is entered.
func (s *BaseDb2ParserListener) EnterSome_any_all(ctx *Some_any_allContext) {}

// ExitSome_any_all is called when production some_any_all is exited.
func (s *BaseDb2ParserListener) ExitSome_any_all(ctx *Some_any_allContext) {}

// EnterRow_value_expression is called when production row_value_expression is entered.
func (s *BaseDb2ParserListener) EnterRow_value_expression(ctx *Row_value_expressionContext) {}

// ExitRow_value_expression is called when production row_value_expression is exited.
func (s *BaseDb2ParserListener) ExitRow_value_expression(ctx *Row_value_expressionContext) {}

// EnterComparison_operator is called when production comparison_operator is entered.
func (s *BaseDb2ParserListener) EnterComparison_operator(ctx *Comparison_operatorContext) {}

// ExitComparison_operator is called when production comparison_operator is exited.
func (s *BaseDb2ParserListener) ExitComparison_operator(ctx *Comparison_operatorContext) {}

// EnterRow_expression is called when production row_expression is entered.
func (s *BaseDb2ParserListener) EnterRow_expression(ctx *Row_expressionContext) {}

// ExitRow_expression is called when production row_expression is exited.
func (s *BaseDb2ParserListener) ExitRow_expression(ctx *Row_expressionContext) {}

// EnterPath_opt_list is called when production path_opt_list is entered.
func (s *BaseDb2ParserListener) EnterPath_opt_list(ctx *Path_opt_listContext) {}

// ExitPath_opt_list is called when production path_opt_list is exited.
func (s *BaseDb2ParserListener) ExitPath_opt_list(ctx *Path_opt_listContext) {}

// EnterPath_opt is called when production path_opt is entered.
func (s *BaseDb2ParserListener) EnterPath_opt(ctx *Path_optContext) {}

// ExitPath_opt is called when production path_opt is exited.
func (s *BaseDb2ParserListener) ExitPath_opt(ctx *Path_optContext) {}

// EnterPkg_opt_list is called when production pkg_opt_list is entered.
func (s *BaseDb2ParserListener) EnterPkg_opt_list(ctx *Pkg_opt_listContext) {}

// ExitPkg_opt_list is called when production pkg_opt_list is exited.
func (s *BaseDb2ParserListener) ExitPkg_opt_list(ctx *Pkg_opt_listContext) {}

// EnterPkg_opt is called when production pkg_opt is entered.
func (s *BaseDb2ParserListener) EnterPkg_opt(ctx *Pkg_optContext) {}

// ExitPkg_opt is called when production pkg_opt is exited.
func (s *BaseDb2ParserListener) ExitPkg_opt(ctx *Pkg_optContext) {}

// EnterMaintain_opt_list is called when production maintain_opt_list is entered.
func (s *BaseDb2ParserListener) EnterMaintain_opt_list(ctx *Maintain_opt_listContext) {}

// ExitMaintain_opt_list is called when production maintain_opt_list is exited.
func (s *BaseDb2ParserListener) ExitMaintain_opt_list(ctx *Maintain_opt_listContext) {}

// EnterMaintain_opt is called when production maintain_opt is entered.
func (s *BaseDb2ParserListener) EnterMaintain_opt(ctx *Maintain_optContext) {}

// ExitMaintain_opt is called when production maintain_opt is exited.
func (s *BaseDb2ParserListener) ExitMaintain_opt(ctx *Maintain_optContext) {}

// EnterVariable is called when production variable is entered.
func (s *BaseDb2ParserListener) EnterVariable(ctx *VariableContext) {}

// ExitVariable is called when production variable is exited.
func (s *BaseDb2ParserListener) ExitVariable(ctx *VariableContext) {}

// EnterHost_variable is called when production host_variable is entered.
func (s *BaseDb2ParserListener) EnterHost_variable(ctx *Host_variableContext) {}

// ExitHost_variable is called when production host_variable is exited.
func (s *BaseDb2ParserListener) ExitHost_variable(ctx *Host_variableContext) {}

// EnterSet_integrity_statement is called when production set_integrity_statement is entered.
func (s *BaseDb2ParserListener) EnterSet_integrity_statement(ctx *Set_integrity_statementContext) {}

// ExitSet_integrity_statement is called when production set_integrity_statement is exited.
func (s *BaseDb2ParserListener) ExitSet_integrity_statement(ctx *Set_integrity_statementContext) {}

// EnterTransfer_ownership_statement is called when production transfer_ownership_statement is entered.
func (s *BaseDb2ParserListener) EnterTransfer_ownership_statement(ctx *Transfer_ownership_statementContext) {
}

// ExitTransfer_ownership_statement is called when production transfer_ownership_statement is exited.
func (s *BaseDb2ParserListener) ExitTransfer_ownership_statement(ctx *Transfer_ownership_statementContext) {
}

// EnterObjects is called when production objects is entered.
func (s *BaseDb2ParserListener) EnterObjects(ctx *ObjectsContext) {}

// ExitObjects is called when production objects is exited.
func (s *BaseDb2ParserListener) ExitObjects(ctx *ObjectsContext) {}

// EnterWhenever_statement is called when production whenever_statement is entered.
func (s *BaseDb2ParserListener) EnterWhenever_statement(ctx *Whenever_statementContext) {}

// ExitWhenever_statement is called when production whenever_statement is exited.
func (s *BaseDb2ParserListener) ExitWhenever_statement(ctx *Whenever_statementContext) {}

// EnterFor_statement is called when production for_statement is entered.
func (s *BaseDb2ParserListener) EnterFor_statement(ctx *For_statementContext) {}

// ExitFor_statement is called when production for_statement is exited.
func (s *BaseDb2ParserListener) ExitFor_statement(ctx *For_statementContext) {}

// EnterGoto_statement is called when production goto_statement is entered.
func (s *BaseDb2ParserListener) EnterGoto_statement(ctx *Goto_statementContext) {}

// ExitGoto_statement is called when production goto_statement is exited.
func (s *BaseDb2ParserListener) ExitGoto_statement(ctx *Goto_statementContext) {}

// EnterIf_statement is called when production if_statement is entered.
func (s *BaseDb2ParserListener) EnterIf_statement(ctx *If_statementContext) {}

// ExitIf_statement is called when production if_statement is exited.
func (s *BaseDb2ParserListener) ExitIf_statement(ctx *If_statementContext) {}

// EnterInclude_statement is called when production include_statement is entered.
func (s *BaseDb2ParserListener) EnterInclude_statement(ctx *Include_statementContext) {}

// ExitInclude_statement is called when production include_statement is exited.
func (s *BaseDb2ParserListener) ExitInclude_statement(ctx *Include_statementContext) {}

// EnterResignal_statement is called when production resignal_statement is entered.
func (s *BaseDb2ParserListener) EnterResignal_statement(ctx *Resignal_statementContext) {}

// ExitResignal_statement is called when production resignal_statement is exited.
func (s *BaseDb2ParserListener) ExitResignal_statement(ctx *Resignal_statementContext) {}

// EnterSignal_information is called when production signal_information is entered.
func (s *BaseDb2ParserListener) EnterSignal_information(ctx *Signal_informationContext) {}

// ExitSignal_information is called when production signal_information is exited.
func (s *BaseDb2ParserListener) ExitSignal_information(ctx *Signal_informationContext) {}

// EnterDiagnostic_string_constant is called when production diagnostic_string_constant is entered.
func (s *BaseDb2ParserListener) EnterDiagnostic_string_constant(ctx *Diagnostic_string_constantContext) {
}

// ExitDiagnostic_string_constant is called when production diagnostic_string_constant is exited.
func (s *BaseDb2ParserListener) ExitDiagnostic_string_constant(ctx *Diagnostic_string_constantContext) {
}

// EnterSignal_statement is called when production signal_statement is entered.
func (s *BaseDb2ParserListener) EnterSignal_statement(ctx *Signal_statementContext) {}

// ExitSignal_statement is called when production signal_statement is exited.
func (s *BaseDb2ParserListener) ExitSignal_statement(ctx *Signal_statementContext) {}

// EnterSqlstate_string_constant is called when production sqlstate_string_constant is entered.
func (s *BaseDb2ParserListener) EnterSqlstate_string_constant(ctx *Sqlstate_string_constantContext) {}

// ExitSqlstate_string_constant is called when production sqlstate_string_constant is exited.
func (s *BaseDb2ParserListener) ExitSqlstate_string_constant(ctx *Sqlstate_string_constantContext) {}

// EnterSqlstate_string_variable is called when production sqlstate_string_variable is entered.
func (s *BaseDb2ParserListener) EnterSqlstate_string_variable(ctx *Sqlstate_string_variableContext) {}

// ExitSqlstate_string_variable is called when production sqlstate_string_variable is exited.
func (s *BaseDb2ParserListener) ExitSqlstate_string_variable(ctx *Sqlstate_string_variableContext) {}

// EnterSignal_information_2 is called when production signal_information_2 is entered.
func (s *BaseDb2ParserListener) EnterSignal_information_2(ctx *Signal_information_2Context) {}

// ExitSignal_information_2 is called when production signal_information_2 is exited.
func (s *BaseDb2ParserListener) ExitSignal_information_2(ctx *Signal_information_2Context) {}

// EnterDiagnostic_string_expression is called when production diagnostic_string_expression is entered.
func (s *BaseDb2ParserListener) EnterDiagnostic_string_expression(ctx *Diagnostic_string_expressionContext) {
}

// ExitDiagnostic_string_expression is called when production diagnostic_string_expression is exited.
func (s *BaseDb2ParserListener) ExitDiagnostic_string_expression(ctx *Diagnostic_string_expressionContext) {
}

// EnterIterate_statement is called when production iterate_statement is entered.
func (s *BaseDb2ParserListener) EnterIterate_statement(ctx *Iterate_statementContext) {}

// ExitIterate_statement is called when production iterate_statement is exited.
func (s *BaseDb2ParserListener) ExitIterate_statement(ctx *Iterate_statementContext) {}

// EnterLeave_statement is called when production leave_statement is entered.
func (s *BaseDb2ParserListener) EnterLeave_statement(ctx *Leave_statementContext) {}

// ExitLeave_statement is called when production leave_statement is exited.
func (s *BaseDb2ParserListener) ExitLeave_statement(ctx *Leave_statementContext) {}

// EnterLoop_statement is called when production loop_statement is entered.
func (s *BaseDb2ParserListener) EnterLoop_statement(ctx *Loop_statementContext) {}

// ExitLoop_statement is called when production loop_statement is exited.
func (s *BaseDb2ParserListener) ExitLoop_statement(ctx *Loop_statementContext) {}

// EnterOpen_statement is called when production open_statement is entered.
func (s *BaseDb2ParserListener) EnterOpen_statement(ctx *Open_statementContext) {}

// ExitOpen_statement is called when production open_statement is exited.
func (s *BaseDb2ParserListener) ExitOpen_statement(ctx *Open_statementContext) {}

// EnterVariable_or_expression is called when production variable_or_expression is entered.
func (s *BaseDb2ParserListener) EnterVariable_or_expression(ctx *Variable_or_expressionContext) {}

// ExitVariable_or_expression is called when production variable_or_expression is exited.
func (s *BaseDb2ParserListener) ExitVariable_or_expression(ctx *Variable_or_expressionContext) {}

// EnterSelect_into_statement is called when production select_into_statement is entered.
func (s *BaseDb2ParserListener) EnterSelect_into_statement(ctx *Select_into_statementContext) {}

// ExitSelect_into_statement is called when production select_into_statement is exited.
func (s *BaseDb2ParserListener) ExitSelect_into_statement(ctx *Select_into_statementContext) {}

// EnterValues_into_statement is called when production values_into_statement is entered.
func (s *BaseDb2ParserListener) EnterValues_into_statement(ctx *Values_into_statementContext) {}

// ExitValues_into_statement is called when production values_into_statement is exited.
func (s *BaseDb2ParserListener) ExitValues_into_statement(ctx *Values_into_statementContext) {}

// EnterPrepare_statement is called when production prepare_statement is entered.
func (s *BaseDb2ParserListener) EnterPrepare_statement(ctx *Prepare_statementContext) {}

// ExitPrepare_statement is called when production prepare_statement is exited.
func (s *BaseDb2ParserListener) ExitPrepare_statement(ctx *Prepare_statementContext) {}

// EnterRepeat_statement is called when production repeat_statement is entered.
func (s *BaseDb2ParserListener) EnterRepeat_statement(ctx *Repeat_statementContext) {}

// ExitRepeat_statement is called when production repeat_statement is exited.
func (s *BaseDb2ParserListener) ExitRepeat_statement(ctx *Repeat_statementContext) {}

// EnterReturn_statement is called when production return_statement is entered.
func (s *BaseDb2ParserListener) EnterReturn_statement(ctx *Return_statementContext) {}

// ExitReturn_statement is called when production return_statement is exited.
func (s *BaseDb2ParserListener) ExitReturn_statement(ctx *Return_statementContext) {}

// EnterWhile_statement is called when production while_statement is entered.
func (s *BaseDb2ParserListener) EnterWhile_statement(ctx *While_statementContext) {}

// ExitWhile_statement is called when production while_statement is exited.
func (s *BaseDb2ParserListener) ExitWhile_statement(ctx *While_statementContext) {}

// EnterSql_routine_statement is called when production sql_routine_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_routine_statement(ctx *Sql_routine_statementContext) {}

// ExitSql_routine_statement is called when production sql_routine_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_routine_statement(ctx *Sql_routine_statementContext) {}

// EnterCommon_table_expression is called when production common_table_expression is entered.
func (s *BaseDb2ParserListener) EnterCommon_table_expression(ctx *Common_table_expressionContext) {}

// ExitCommon_table_expression is called when production common_table_expression is exited.
func (s *BaseDb2ParserListener) ExitCommon_table_expression(ctx *Common_table_expressionContext) {}

// EnterCreate_alias_statement is called when production create_alias_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_alias_statement(ctx *Create_alias_statementContext) {}

// ExitCreate_alias_statement is called when production create_alias_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_alias_statement(ctx *Create_alias_statementContext) {}

// EnterTable_alias is called when production table_alias is entered.
func (s *BaseDb2ParserListener) EnterTable_alias(ctx *Table_aliasContext) {}

// ExitTable_alias is called when production table_alias is exited.
func (s *BaseDb2ParserListener) ExitTable_alias(ctx *Table_aliasContext) {}

// EnterModule_alias is called when production module_alias is entered.
func (s *BaseDb2ParserListener) EnterModule_alias(ctx *Module_aliasContext) {}

// ExitModule_alias is called when production module_alias is exited.
func (s *BaseDb2ParserListener) ExitModule_alias(ctx *Module_aliasContext) {}

// EnterSequence_alias is called when production sequence_alias is entered.
func (s *BaseDb2ParserListener) EnterSequence_alias(ctx *Sequence_aliasContext) {}

// ExitSequence_alias is called when production sequence_alias is exited.
func (s *BaseDb2ParserListener) ExitSequence_alias(ctx *Sequence_aliasContext) {}

// EnterOr_replace is called when production or_replace is entered.
func (s *BaseDb2ParserListener) EnterOr_replace(ctx *Or_replaceContext) {}

// ExitOr_replace is called when production or_replace is exited.
func (s *BaseDb2ParserListener) ExitOr_replace(ctx *Or_replaceContext) {}

// EnterCreate_audit_policy_statement is called when production create_audit_policy_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_audit_policy_statement(ctx *Create_audit_policy_statementContext) {
}

// ExitCreate_audit_policy_statement is called when production create_audit_policy_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_audit_policy_statement(ctx *Create_audit_policy_statementContext) {
}

// EnterAudit_policy_opts is called when production audit_policy_opts is entered.
func (s *BaseDb2ParserListener) EnterAudit_policy_opts(ctx *Audit_policy_optsContext) {}

// ExitAudit_policy_opts is called when production audit_policy_opts is exited.
func (s *BaseDb2ParserListener) ExitAudit_policy_opts(ctx *Audit_policy_optsContext) {}

// EnterAudit_policy_categories_opts is called when production audit_policy_categories_opts is entered.
func (s *BaseDb2ParserListener) EnterAudit_policy_categories_opts(ctx *Audit_policy_categories_optsContext) {
}

// ExitAudit_policy_categories_opts is called when production audit_policy_categories_opts is exited.
func (s *BaseDb2ParserListener) ExitAudit_policy_categories_opts(ctx *Audit_policy_categories_optsContext) {
}

// EnterCreate_bufferpool_statement is called when production create_bufferpool_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_bufferpool_statement(ctx *Create_bufferpool_statementContext) {
}

// ExitCreate_bufferpool_statement is called when production create_bufferpool_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_bufferpool_statement(ctx *Create_bufferpool_statementContext) {
}

// EnterBufferpool_opts is called when production bufferpool_opts is entered.
func (s *BaseDb2ParserListener) EnterBufferpool_opts(ctx *Bufferpool_optsContext) {}

// ExitBufferpool_opts is called when production bufferpool_opts is exited.
func (s *BaseDb2ParserListener) ExitBufferpool_opts(ctx *Bufferpool_optsContext) {}

// EnterExcept_clause is called when production except_clause is entered.
func (s *BaseDb2ParserListener) EnterExcept_clause(ctx *Except_clauseContext) {}

// ExitExcept_clause is called when production except_clause is exited.
func (s *BaseDb2ParserListener) ExitExcept_clause(ctx *Except_clauseContext) {}

// EnterMember_list is called when production member_list is entered.
func (s *BaseDb2ParserListener) EnterMember_list(ctx *Member_listContext) {}

// ExitMember_list is called when production member_list is exited.
func (s *BaseDb2ParserListener) ExitMember_list(ctx *Member_listContext) {}

// EnterMember_list_item is called when production member_list_item is entered.
func (s *BaseDb2ParserListener) EnterMember_list_item(ctx *Member_list_itemContext) {}

// ExitMember_list_item is called when production member_list_item is exited.
func (s *BaseDb2ParserListener) ExitMember_list_item(ctx *Member_list_itemContext) {}

// EnterCreate_database_partition_group_statement is called when production create_database_partition_group_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_database_partition_group_statement(ctx *Create_database_partition_group_statementContext) {
}

// ExitCreate_database_partition_group_statement is called when production create_database_partition_group_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_database_partition_group_statement(ctx *Create_database_partition_group_statementContext) {
}

// EnterCreate_event_monitor_statement is called when production create_event_monitor_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_event_monitor_statement(ctx *Create_event_monitor_statementContext) {
}

// ExitCreate_event_monitor_statement is called when production create_event_monitor_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_event_monitor_statement(ctx *Create_event_monitor_statementContext) {
}

// EnterCreate_event_monitor_activities_statement is called when production create_event_monitor_activities_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_event_monitor_activities_statement(ctx *Create_event_monitor_activities_statementContext) {
}

// ExitCreate_event_monitor_activities_statement is called when production create_event_monitor_activities_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_event_monitor_activities_statement(ctx *Create_event_monitor_activities_statementContext) {
}

// EnterFormatted_event_table_info_3 is called when production formatted_event_table_info_3 is entered.
func (s *BaseDb2ParserListener) EnterFormatted_event_table_info_3(ctx *Formatted_event_table_info_3Context) {
}

// ExitFormatted_event_table_info_3 is called when production formatted_event_table_info_3 is exited.
func (s *BaseDb2ParserListener) ExitFormatted_event_table_info_3(ctx *Formatted_event_table_info_3Context) {
}

// EnterCreate_event_monitor_change_history_statement is called when production create_event_monitor_change_history_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_event_monitor_change_history_statement(ctx *Create_event_monitor_change_history_statementContext) {
}

// ExitCreate_event_monitor_change_history_statement is called when production create_event_monitor_change_history_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_event_monitor_change_history_statement(ctx *Create_event_monitor_change_history_statementContext) {
}

// EnterEvent_control_list is called when production event_control_list is entered.
func (s *BaseDb2ParserListener) EnterEvent_control_list(ctx *Event_control_listContext) {}

// ExitEvent_control_list is called when production event_control_list is exited.
func (s *BaseDb2ParserListener) ExitEvent_control_list(ctx *Event_control_listContext) {}

// EnterEvent_control is called when production event_control is entered.
func (s *BaseDb2ParserListener) EnterEvent_control(ctx *Event_controlContext) {}

// ExitEvent_control is called when production event_control is exited.
func (s *BaseDb2ParserListener) ExitEvent_control(ctx *Event_controlContext) {}

// EnterCreate_event_monitor_locking_statement is called when production create_event_monitor_locking_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_event_monitor_locking_statement(ctx *Create_event_monitor_locking_statementContext) {
}

// ExitCreate_event_monitor_locking_statement is called when production create_event_monitor_locking_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_event_monitor_locking_statement(ctx *Create_event_monitor_locking_statementContext) {
}

// EnterCreate_event_monitor_package_cache_statement is called when production create_event_monitor_package_cache_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_event_monitor_package_cache_statement(ctx *Create_event_monitor_package_cache_statementContext) {
}

// ExitCreate_event_monitor_package_cache_statement is called when production create_event_monitor_package_cache_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_event_monitor_package_cache_statement(ctx *Create_event_monitor_package_cache_statementContext) {
}

// EnterFilter_and_collection_options is called when production filter_and_collection_options is entered.
func (s *BaseDb2ParserListener) EnterFilter_and_collection_options(ctx *Filter_and_collection_optionsContext) {
}

// ExitFilter_and_collection_options is called when production filter_and_collection_options is exited.
func (s *BaseDb2ParserListener) ExitFilter_and_collection_options(ctx *Filter_and_collection_optionsContext) {
}

// EnterEvent_condition is called when production event_condition is entered.
func (s *BaseDb2ParserListener) EnterEvent_condition(ctx *Event_conditionContext) {}

// ExitEvent_condition is called when production event_condition is exited.
func (s *BaseDb2ParserListener) ExitEvent_condition(ctx *Event_conditionContext) {}

// EnterEvent_condition_item is called when production event_condition_item is entered.
func (s *BaseDb2ParserListener) EnterEvent_condition_item(ctx *Event_condition_itemContext) {}

// ExitEvent_condition_item is called when production event_condition_item is exited.
func (s *BaseDb2ParserListener) ExitEvent_condition_item(ctx *Event_condition_itemContext) {}

// EnterCreate_event_monitor_statistics_statement is called when production create_event_monitor_statistics_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_event_monitor_statistics_statement(ctx *Create_event_monitor_statistics_statementContext) {
}

// ExitCreate_event_monitor_statistics_statement is called when production create_event_monitor_statistics_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_event_monitor_statistics_statement(ctx *Create_event_monitor_statistics_statementContext) {
}

// EnterEvent_monitor_statistics_opts is called when production event_monitor_statistics_opts is entered.
func (s *BaseDb2ParserListener) EnterEvent_monitor_statistics_opts(ctx *Event_monitor_statistics_optsContext) {
}

// ExitEvent_monitor_statistics_opts is called when production event_monitor_statistics_opts is exited.
func (s *BaseDb2ParserListener) ExitEvent_monitor_statistics_opts(ctx *Event_monitor_statistics_optsContext) {
}

// EnterCreate_event_monitor_threshold_violations_statement is called when production create_event_monitor_threshold_violations_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_event_monitor_threshold_violations_statement(ctx *Create_event_monitor_threshold_violations_statementContext) {
}

// ExitCreate_event_monitor_threshold_violations_statement is called when production create_event_monitor_threshold_violations_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_event_monitor_threshold_violations_statement(ctx *Create_event_monitor_threshold_violations_statementContext) {
}

// EnterFormatted_event_table_info_2 is called when production formatted_event_table_info_2 is entered.
func (s *BaseDb2ParserListener) EnterFormatted_event_table_info_2(ctx *Formatted_event_table_info_2Context) {
}

// ExitFormatted_event_table_info_2 is called when production formatted_event_table_info_2 is exited.
func (s *BaseDb2ParserListener) ExitFormatted_event_table_info_2(ctx *Formatted_event_table_info_2Context) {
}

// EnterFile_options is called when production file_options is entered.
func (s *BaseDb2ParserListener) EnterFile_options(ctx *File_optionsContext) {}

// ExitFile_options is called when production file_options is exited.
func (s *BaseDb2ParserListener) ExitFile_options(ctx *File_optionsContext) {}

// EnterEvent_monitor_threshold_opts is called when production event_monitor_threshold_opts is entered.
func (s *BaseDb2ParserListener) EnterEvent_monitor_threshold_opts(ctx *Event_monitor_threshold_optsContext) {
}

// ExitEvent_monitor_threshold_opts is called when production event_monitor_threshold_opts is exited.
func (s *BaseDb2ParserListener) ExitEvent_monitor_threshold_opts(ctx *Event_monitor_threshold_optsContext) {
}

// EnterPages is called when production pages is entered.
func (s *BaseDb2ParserListener) EnterPages(ctx *PagesContext) {}

// ExitPages is called when production pages is exited.
func (s *BaseDb2ParserListener) ExitPages(ctx *PagesContext) {}

// EnterCreate_event_monitor_unit_of_work is called when production create_event_monitor_unit_of_work is entered.
func (s *BaseDb2ParserListener) EnterCreate_event_monitor_unit_of_work(ctx *Create_event_monitor_unit_of_workContext) {
}

// ExitCreate_event_monitor_unit_of_work is called when production create_event_monitor_unit_of_work is exited.
func (s *BaseDb2ParserListener) ExitCreate_event_monitor_unit_of_work(ctx *Create_event_monitor_unit_of_workContext) {
}

// EnterFormatted_event_table_info is called when production formatted_event_table_info is entered.
func (s *BaseDb2ParserListener) EnterFormatted_event_table_info(ctx *Formatted_event_table_infoContext) {
}

// ExitFormatted_event_table_info is called when production formatted_event_table_info is exited.
func (s *BaseDb2ParserListener) ExitFormatted_event_table_info(ctx *Formatted_event_table_infoContext) {
}

// EnterAutostart_manualstart is called when production autostart_manualstart is entered.
func (s *BaseDb2ParserListener) EnterAutostart_manualstart(ctx *Autostart_manualstartContext) {}

// ExitAutostart_manualstart is called when production autostart_manualstart is exited.
func (s *BaseDb2ParserListener) ExitAutostart_manualstart(ctx *Autostart_manualstartContext) {}

// EnterEvm_group is called when production evm_group is entered.
func (s *BaseDb2ParserListener) EnterEvm_group(ctx *Evm_groupContext) {}

// ExitEvm_group is called when production evm_group is exited.
func (s *BaseDb2ParserListener) ExitEvm_group(ctx *Evm_groupContext) {}

// EnterTarget_table_options is called when production target_table_options is entered.
func (s *BaseDb2ParserListener) EnterTarget_table_options(ctx *Target_table_optionsContext) {}

// ExitTarget_table_options is called when production target_table_options is exited.
func (s *BaseDb2ParserListener) ExitTarget_table_options(ctx *Target_table_optionsContext) {}

// EnterCreate_external_table_statement is called when production create_external_table_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_external_table_statement(ctx *Create_external_table_statementContext) {
}

// ExitCreate_external_table_statement is called when production create_external_table_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_external_table_statement(ctx *Create_external_table_statementContext) {
}

// EnterExt_table_option is called when production ext_table_option is entered.
func (s *BaseDb2ParserListener) EnterExt_table_option(ctx *Ext_table_optionContext) {}

// ExitExt_table_option is called when production ext_table_option is exited.
func (s *BaseDb2ParserListener) ExitExt_table_option(ctx *Ext_table_optionContext) {}

// EnterExt_table_option_value is called when production ext_table_option_value is entered.
func (s *BaseDb2ParserListener) EnterExt_table_option_value(ctx *Ext_table_option_valueContext) {}

// ExitExt_table_option_value is called when production ext_table_option_value is exited.
func (s *BaseDb2ParserListener) ExitExt_table_option_value(ctx *Ext_table_option_valueContext) {}

// EnterCreate_function_statement is called when production create_function_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_function_statement(ctx *Create_function_statementContext) {
}

// ExitCreate_function_statement is called when production create_function_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_function_statement(ctx *Create_function_statementContext) {
}

// EnterCreate_function_aggregate_interface_statement is called when production create_function_aggregate_interface_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_function_aggregate_interface_statement(ctx *Create_function_aggregate_interface_statementContext) {
}

// ExitCreate_function_aggregate_interface_statement is called when production create_function_aggregate_interface_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_function_aggregate_interface_statement(ctx *Create_function_aggregate_interface_statementContext) {
}

// EnterAgg_fn_param_decl is called when production agg_fn_param_decl is entered.
func (s *BaseDb2ParserListener) EnterAgg_fn_param_decl(ctx *Agg_fn_param_declContext) {}

// ExitAgg_fn_param_decl is called when production agg_fn_param_decl is exited.
func (s *BaseDb2ParserListener) ExitAgg_fn_param_decl(ctx *Agg_fn_param_declContext) {}

// EnterAgg_fn_option_list is called when production agg_fn_option_list is entered.
func (s *BaseDb2ParserListener) EnterAgg_fn_option_list(ctx *Agg_fn_option_listContext) {}

// ExitAgg_fn_option_list is called when production agg_fn_option_list is exited.
func (s *BaseDb2ParserListener) ExitAgg_fn_option_list(ctx *Agg_fn_option_listContext) {}

// EnterState_variable_declaration is called when production state_variable_declaration is entered.
func (s *BaseDb2ParserListener) EnterState_variable_declaration(ctx *State_variable_declarationContext) {
}

// ExitState_variable_declaration is called when production state_variable_declaration is exited.
func (s *BaseDb2ParserListener) ExitState_variable_declaration(ctx *State_variable_declarationContext) {
}

// EnterCreate_function_external_scalar_statement is called when production create_function_external_scalar_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_function_external_scalar_statement(ctx *Create_function_external_scalar_statementContext) {
}

// ExitCreate_function_external_scalar_statement is called when production create_function_external_scalar_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_function_external_scalar_statement(ctx *Create_function_external_scalar_statementContext) {
}

// EnterExt_scalar_param_decl is called when production ext_scalar_param_decl is entered.
func (s *BaseDb2ParserListener) EnterExt_scalar_param_decl(ctx *Ext_scalar_param_declContext) {}

// ExitExt_scalar_param_decl is called when production ext_scalar_param_decl is exited.
func (s *BaseDb2ParserListener) ExitExt_scalar_param_decl(ctx *Ext_scalar_param_declContext) {}

// EnterExt_scalar_option_list is called when production ext_scalar_option_list is entered.
func (s *BaseDb2ParserListener) EnterExt_scalar_option_list(ctx *Ext_scalar_option_listContext) {}

// ExitExt_scalar_option_list is called when production ext_scalar_option_list is exited.
func (s *BaseDb2ParserListener) ExitExt_scalar_option_list(ctx *Ext_scalar_option_listContext) {}

// EnterExt_scalar_option_list_item is called when production ext_scalar_option_list_item is entered.
func (s *BaseDb2ParserListener) EnterExt_scalar_option_list_item(ctx *Ext_scalar_option_list_itemContext) {
}

// ExitExt_scalar_option_list_item is called when production ext_scalar_option_list_item is exited.
func (s *BaseDb2ParserListener) ExitExt_scalar_option_list_item(ctx *Ext_scalar_option_list_itemContext) {
}

// EnterPredicate_specification is called when production predicate_specification is entered.
func (s *BaseDb2ParserListener) EnterPredicate_specification(ctx *Predicate_specificationContext) {}

// ExitPredicate_specification is called when production predicate_specification is exited.
func (s *BaseDb2ParserListener) ExitPredicate_specification(ctx *Predicate_specificationContext) {}

// EnterData_filter is called when production data_filter is entered.
func (s *BaseDb2ParserListener) EnterData_filter(ctx *Data_filterContext) {}

// ExitData_filter is called when production data_filter is exited.
func (s *BaseDb2ParserListener) ExitData_filter(ctx *Data_filterContext) {}

// EnterIndex_exploitation is called when production index_exploitation is entered.
func (s *BaseDb2ParserListener) EnterIndex_exploitation(ctx *Index_exploitationContext) {}

// ExitIndex_exploitation is called when production index_exploitation is exited.
func (s *BaseDb2ParserListener) ExitIndex_exploitation(ctx *Index_exploitationContext) {}

// EnterExploitation_rule is called when production exploitation_rule is entered.
func (s *BaseDb2ParserListener) EnterExploitation_rule(ctx *Exploitation_ruleContext) {}

// ExitExploitation_rule is called when production exploitation_rule is exited.
func (s *BaseDb2ParserListener) ExitExploitation_rule(ctx *Exploitation_ruleContext) {}

// EnterCreate_function_external_table_statement is called when production create_function_external_table_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_function_external_table_statement(ctx *Create_function_external_table_statementContext) {
}

// ExitCreate_function_external_table_statement is called when production create_function_external_table_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_function_external_table_statement(ctx *Create_function_external_table_statementContext) {
}

// EnterExt_table_param_decl_list is called when production ext_table_param_decl_list is entered.
func (s *BaseDb2ParserListener) EnterExt_table_param_decl_list(ctx *Ext_table_param_decl_listContext) {
}

// ExitExt_table_param_decl_list is called when production ext_table_param_decl_list is exited.
func (s *BaseDb2ParserListener) ExitExt_table_param_decl_list(ctx *Ext_table_param_decl_listContext) {
}

// EnterExt_table_param_decl is called when production ext_table_param_decl is entered.
func (s *BaseDb2ParserListener) EnterExt_table_param_decl(ctx *Ext_table_param_declContext) {}

// ExitExt_table_param_decl is called when production ext_table_param_decl is exited.
func (s *BaseDb2ParserListener) ExitExt_table_param_decl(ctx *Ext_table_param_declContext) {}

// EnterExt_table_option_list is called when production ext_table_option_list is entered.
func (s *BaseDb2ParserListener) EnterExt_table_option_list(ctx *Ext_table_option_listContext) {}

// ExitExt_table_option_list is called when production ext_table_option_list is exited.
func (s *BaseDb2ParserListener) ExitExt_table_option_list(ctx *Ext_table_option_listContext) {}

// EnterExt_table_option_list_item is called when production ext_table_option_list_item is entered.
func (s *BaseDb2ParserListener) EnterExt_table_option_list_item(ctx *Ext_table_option_list_itemContext) {
}

// ExitExt_table_option_list_item is called when production ext_table_option_list_item is exited.
func (s *BaseDb2ParserListener) ExitExt_table_option_list_item(ctx *Ext_table_option_list_itemContext) {
}

// EnterCreate_function_old_db_external_function_statement is called when production create_function_old_db_external_function_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_function_old_db_external_function_statement(ctx *Create_function_old_db_external_function_statementContext) {
}

// ExitCreate_function_old_db_external_function_statement is called when production create_function_old_db_external_function_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_function_old_db_external_function_statement(ctx *Create_function_old_db_external_function_statementContext) {
}

// EnterOledb_option_list is called when production oledb_option_list is entered.
func (s *BaseDb2ParserListener) EnterOledb_option_list(ctx *Oledb_option_listContext) {}

// ExitOledb_option_list is called when production oledb_option_list is exited.
func (s *BaseDb2ParserListener) ExitOledb_option_list(ctx *Oledb_option_listContext) {}

// EnterOledb_option_list_item is called when production oledb_option_list_item is entered.
func (s *BaseDb2ParserListener) EnterOledb_option_list_item(ctx *Oledb_option_list_itemContext) {}

// ExitOledb_option_list_item is called when production oledb_option_list_item is exited.
func (s *BaseDb2ParserListener) ExitOledb_option_list_item(ctx *Oledb_option_list_itemContext) {}

// EnterCreate_function_sourced_or_template_statement is called when production create_function_sourced_or_template_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_function_sourced_or_template_statement(ctx *Create_function_sourced_or_template_statementContext) {
}

// ExitCreate_function_sourced_or_template_statement is called when production create_function_sourced_or_template_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_function_sourced_or_template_statement(ctx *Create_function_sourced_or_template_statementContext) {
}

// EnterFn_return_opts is called when production fn_return_opts is entered.
func (s *BaseDb2ParserListener) EnterFn_return_opts(ctx *Fn_return_optsContext) {}

// ExitFn_return_opts is called when production fn_return_opts is exited.
func (s *BaseDb2ParserListener) ExitFn_return_opts(ctx *Fn_return_optsContext) {}

// EnterFn_return_opts_item is called when production fn_return_opts_item is entered.
func (s *BaseDb2ParserListener) EnterFn_return_opts_item(ctx *Fn_return_opts_itemContext) {}

// ExitFn_return_opts_item is called when production fn_return_opts_item is exited.
func (s *BaseDb2ParserListener) ExitFn_return_opts_item(ctx *Fn_return_opts_itemContext) {}

// EnterTemplate_opts is called when production template_opts is entered.
func (s *BaseDb2ParserListener) EnterTemplate_opts(ctx *Template_optsContext) {}

// ExitTemplate_opts is called when production template_opts is exited.
func (s *BaseDb2ParserListener) ExitTemplate_opts(ctx *Template_optsContext) {}

// EnterTemplate_opts_item is called when production template_opts_item is entered.
func (s *BaseDb2ParserListener) EnterTemplate_opts_item(ctx *Template_opts_itemContext) {}

// ExitTemplate_opts_item is called when production template_opts_item is exited.
func (s *BaseDb2ParserListener) ExitTemplate_opts_item(ctx *Template_opts_itemContext) {}

// EnterAscii_unicode is called when production ascii_unicode is entered.
func (s *BaseDb2ParserListener) EnterAscii_unicode(ctx *Ascii_unicodeContext) {}

// ExitAscii_unicode is called when production ascii_unicode is exited.
func (s *BaseDb2ParserListener) ExitAscii_unicode(ctx *Ascii_unicodeContext) {}

// EnterParam_decl_list_3 is called when production param_decl_list_3 is entered.
func (s *BaseDb2ParserListener) EnterParam_decl_list_3(ctx *Param_decl_list_3Context) {}

// ExitParam_decl_list_3 is called when production param_decl_list_3 is exited.
func (s *BaseDb2ParserListener) ExitParam_decl_list_3(ctx *Param_decl_list_3Context) {}

// EnterParam_decl_3 is called when production param_decl_3 is entered.
func (s *BaseDb2ParserListener) EnterParam_decl_3(ctx *Param_decl_3Context) {}

// ExitParam_decl_3 is called when production param_decl_3 is exited.
func (s *BaseDb2ParserListener) ExitParam_decl_3(ctx *Param_decl_3Context) {}

// EnterCreate_function_sql_scalar_table_or_row_statement is called when production create_function_sql_scalar_table_or_row_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_function_sql_scalar_table_or_row_statement(ctx *Create_function_sql_scalar_table_or_row_statementContext) {
}

// ExitCreate_function_sql_scalar_table_or_row_statement is called when production create_function_sql_scalar_table_or_row_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_function_sql_scalar_table_or_row_statement(ctx *Create_function_sql_scalar_table_or_row_statementContext) {
}

// EnterParam_decl_list_2 is called when production param_decl_list_2 is entered.
func (s *BaseDb2ParserListener) EnterParam_decl_list_2(ctx *Param_decl_list_2Context) {}

// ExitParam_decl_list_2 is called when production param_decl_list_2 is exited.
func (s *BaseDb2ParserListener) ExitParam_decl_list_2(ctx *Param_decl_list_2Context) {}

// EnterParam_decl_2 is called when production param_decl_2 is entered.
func (s *BaseDb2ParserListener) EnterParam_decl_2(ctx *Param_decl_2Context) {}

// ExitParam_decl_2 is called when production param_decl_2 is exited.
func (s *BaseDb2ParserListener) ExitParam_decl_2(ctx *Param_decl_2Context) {}

// EnterSql_function_body is called when production sql_function_body is entered.
func (s *BaseDb2ParserListener) EnterSql_function_body(ctx *Sql_function_bodyContext) {}

// ExitSql_function_body is called when production sql_function_body is exited.
func (s *BaseDb2ParserListener) ExitSql_function_body(ctx *Sql_function_bodyContext) {}

// EnterCreate_function_mapping_statement is called when production create_function_mapping_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_function_mapping_statement(ctx *Create_function_mapping_statementContext) {
}

// ExitCreate_function_mapping_statement is called when production create_function_mapping_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_function_mapping_statement(ctx *Create_function_mapping_statementContext) {
}

// EnterFunction_options is called when production function_options is entered.
func (s *BaseDb2ParserListener) EnterFunction_options(ctx *Function_optionsContext) {}

// ExitFunction_options is called when production function_options is exited.
func (s *BaseDb2ParserListener) ExitFunction_options(ctx *Function_optionsContext) {}

// EnterFunction_option_name is called when production function_option_name is entered.
func (s *BaseDb2ParserListener) EnterFunction_option_name(ctx *Function_option_nameContext) {}

// ExitFunction_option_name is called when production function_option_name is exited.
func (s *BaseDb2ParserListener) ExitFunction_option_name(ctx *Function_option_nameContext) {}

// EnterCreate_global_temporary_table_statement is called when production create_global_temporary_table_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_global_temporary_table_statement(ctx *Create_global_temporary_table_statementContext) {
}

// ExitCreate_global_temporary_table_statement is called when production create_global_temporary_table_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_global_temporary_table_statement(ctx *Create_global_temporary_table_statementContext) {
}

// EnterCreate_global_temporary_table_opts is called when production create_global_temporary_table_opts is entered.
func (s *BaseDb2ParserListener) EnterCreate_global_temporary_table_opts(ctx *Create_global_temporary_table_optsContext) {
}

// ExitCreate_global_temporary_table_opts is called when production create_global_temporary_table_opts is exited.
func (s *BaseDb2ParserListener) ExitCreate_global_temporary_table_opts(ctx *Create_global_temporary_table_optsContext) {
}

// EnterCreate_global_temporary_table_item is called when production create_global_temporary_table_item is entered.
func (s *BaseDb2ParserListener) EnterCreate_global_temporary_table_item(ctx *Create_global_temporary_table_itemContext) {
}

// ExitCreate_global_temporary_table_item is called when production create_global_temporary_table_item is exited.
func (s *BaseDb2ParserListener) ExitCreate_global_temporary_table_item(ctx *Create_global_temporary_table_itemContext) {
}

// EnterDelete_preserve is called when production delete_preserve is entered.
func (s *BaseDb2ParserListener) EnterDelete_preserve(ctx *Delete_preserveContext) {}

// ExitDelete_preserve is called when production delete_preserve is exited.
func (s *BaseDb2ParserListener) ExitDelete_preserve(ctx *Delete_preserveContext) {}

// EnterCreate_histogram_template_statement is called when production create_histogram_template_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_histogram_template_statement(ctx *Create_histogram_template_statementContext) {
}

// ExitCreate_histogram_template_statement is called when production create_histogram_template_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_histogram_template_statement(ctx *Create_histogram_template_statementContext) {
}

// EnterCreate_index_statement is called when production create_index_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_index_statement(ctx *Create_index_statementContext) {}

// ExitCreate_index_statement is called when production create_index_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_index_statement(ctx *Create_index_statementContext) {}

// EnterIndex_col_opts is called when production index_col_opts is entered.
func (s *BaseDb2ParserListener) EnterIndex_col_opts(ctx *Index_col_optsContext) {}

// ExitIndex_col_opts is called when production index_col_opts is exited.
func (s *BaseDb2ParserListener) ExitIndex_col_opts(ctx *Index_col_optsContext) {}

// EnterIndex_col_opts_item is called when production index_col_opts_item is entered.
func (s *BaseDb2ParserListener) EnterIndex_col_opts_item(ctx *Index_col_opts_itemContext) {}

// ExitIndex_col_opts_item is called when production index_col_opts_item is exited.
func (s *BaseDb2ParserListener) ExitIndex_col_opts_item(ctx *Index_col_opts_itemContext) {}

// EnterKey_expression is called when production key_expression is entered.
func (s *BaseDb2ParserListener) EnterKey_expression(ctx *Key_expressionContext) {}

// ExitKey_expression is called when production key_expression is exited.
func (s *BaseDb2ParserListener) ExitKey_expression(ctx *Key_expressionContext) {}

// EnterCreate_index_extension_statement is called when production create_index_extension_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_index_extension_statement(ctx *Create_index_extension_statementContext) {
}

// ExitCreate_index_extension_statement is called when production create_index_extension_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_index_extension_statement(ctx *Create_index_extension_statementContext) {
}

// EnterParam_list is called when production param_list is entered.
func (s *BaseDb2ParserListener) EnterParam_list(ctx *Param_listContext) {}

// ExitParam_list is called when production param_list is exited.
func (s *BaseDb2ParserListener) ExitParam_list(ctx *Param_listContext) {}

// EnterIndex_maintenance is called when production index_maintenance is entered.
func (s *BaseDb2ParserListener) EnterIndex_maintenance(ctx *Index_maintenanceContext) {}

// ExitIndex_maintenance is called when production index_maintenance is exited.
func (s *BaseDb2ParserListener) ExitIndex_maintenance(ctx *Index_maintenanceContext) {}

// EnterTable_function_invocation is called when production table_function_invocation is entered.
func (s *BaseDb2ParserListener) EnterTable_function_invocation(ctx *Table_function_invocationContext) {
}

// ExitTable_function_invocation is called when production table_function_invocation is exited.
func (s *BaseDb2ParserListener) ExitTable_function_invocation(ctx *Table_function_invocationContext) {
}

// EnterIndex_search is called when production index_search is entered.
func (s *BaseDb2ParserListener) EnterIndex_search(ctx *Index_searchContext) {}

// ExitIndex_search is called when production index_search is exited.
func (s *BaseDb2ParserListener) ExitIndex_search(ctx *Index_searchContext) {}

// EnterSearch_method_definition is called when production search_method_definition is entered.
func (s *BaseDb2ParserListener) EnterSearch_method_definition(ctx *Search_method_definitionContext) {}

// ExitSearch_method_definition is called when production search_method_definition is exited.
func (s *BaseDb2ParserListener) ExitSearch_method_definition(ctx *Search_method_definitionContext) {}

// EnterCreate_mask_statement is called when production create_mask_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_mask_statement(ctx *Create_mask_statementContext) {}

// ExitCreate_mask_statement is called when production create_mask_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_mask_statement(ctx *Create_mask_statementContext) {}

// EnterCase_expression is called when production case_expression is entered.
func (s *BaseDb2ParserListener) EnterCase_expression(ctx *Case_expressionContext) {}

// ExitCase_expression is called when production case_expression is exited.
func (s *BaseDb2ParserListener) ExitCase_expression(ctx *Case_expressionContext) {}

// EnterRange_producing_funciton_invocation is called when production range_producing_funciton_invocation is entered.
func (s *BaseDb2ParserListener) EnterRange_producing_funciton_invocation(ctx *Range_producing_funciton_invocationContext) {
}

// ExitRange_producing_funciton_invocation is called when production range_producing_funciton_invocation is exited.
func (s *BaseDb2ParserListener) ExitRange_producing_funciton_invocation(ctx *Range_producing_funciton_invocationContext) {
}

// EnterIndex_filtering_function_invocation is called when production index_filtering_function_invocation is entered.
func (s *BaseDb2ParserListener) EnterIndex_filtering_function_invocation(ctx *Index_filtering_function_invocationContext) {
}

// ExitIndex_filtering_function_invocation is called when production index_filtering_function_invocation is exited.
func (s *BaseDb2ParserListener) ExitIndex_filtering_function_invocation(ctx *Index_filtering_function_invocationContext) {
}

// EnterCreate_method_statement is called when production create_method_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_method_statement(ctx *Create_method_statementContext) {}

// ExitCreate_method_statement is called when production create_method_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_method_statement(ctx *Create_method_statementContext) {}

// EnterMethod_opts is called when production method_opts is entered.
func (s *BaseDb2ParserListener) EnterMethod_opts(ctx *Method_optsContext) {}

// ExitMethod_opts is called when production method_opts is exited.
func (s *BaseDb2ParserListener) ExitMethod_opts(ctx *Method_optsContext) {}

// EnterMethod_opts_item is called when production method_opts_item is entered.
func (s *BaseDb2ParserListener) EnterMethod_opts_item(ctx *Method_opts_itemContext) {}

// ExitMethod_opts_item is called when production method_opts_item is exited.
func (s *BaseDb2ParserListener) ExitMethod_opts_item(ctx *Method_opts_itemContext) {}

// EnterMethod_signature is called when production method_signature is entered.
func (s *BaseDb2ParserListener) EnterMethod_signature(ctx *Method_signatureContext) {}

// ExitMethod_signature is called when production method_signature is exited.
func (s *BaseDb2ParserListener) ExitMethod_signature(ctx *Method_signatureContext) {}

// EnterMethod_param_list is called when production method_param_list is entered.
func (s *BaseDb2ParserListener) EnterMethod_param_list(ctx *Method_param_listContext) {}

// ExitMethod_param_list is called when production method_param_list is exited.
func (s *BaseDb2ParserListener) ExitMethod_param_list(ctx *Method_param_listContext) {}

// EnterData_type_3 is called when production data_type_3 is entered.
func (s *BaseDb2ParserListener) EnterData_type_3(ctx *Data_type_3Context) {}

// ExitData_type_3 is called when production data_type_3 is exited.
func (s *BaseDb2ParserListener) ExitData_type_3(ctx *Data_type_3Context) {}

// EnterData_type_4 is called when production data_type_4 is entered.
func (s *BaseDb2ParserListener) EnterData_type_4(ctx *Data_type_4Context) {}

// ExitData_type_4 is called when production data_type_4 is exited.
func (s *BaseDb2ParserListener) ExitData_type_4(ctx *Data_type_4Context) {}

// EnterSql_method_body is called when production sql_method_body is entered.
func (s *BaseDb2ParserListener) EnterSql_method_body(ctx *Sql_method_bodyContext) {}

// ExitSql_method_body is called when production sql_method_body is exited.
func (s *BaseDb2ParserListener) ExitSql_method_body(ctx *Sql_method_bodyContext) {}

// EnterCompound_sql_inlined is called when production compound_sql_inlined is entered.
func (s *BaseDb2ParserListener) EnterCompound_sql_inlined(ctx *Compound_sql_inlinedContext) {}

// ExitCompound_sql_inlined is called when production compound_sql_inlined is exited.
func (s *BaseDb2ParserListener) ExitCompound_sql_inlined(ctx *Compound_sql_inlinedContext) {}

// EnterSql_statement_inlined is called when production sql_statement_inlined is entered.
func (s *BaseDb2ParserListener) EnterSql_statement_inlined(ctx *Sql_statement_inlinedContext) {}

// ExitSql_statement_inlined is called when production sql_statement_inlined is exited.
func (s *BaseDb2ParserListener) ExitSql_statement_inlined(ctx *Sql_statement_inlinedContext) {}

// EnterCompound_sql_compiled is called when production compound_sql_compiled is entered.
func (s *BaseDb2ParserListener) EnterCompound_sql_compiled(ctx *Compound_sql_compiledContext) {}

// ExitCompound_sql_compiled is called when production compound_sql_compiled is exited.
func (s *BaseDb2ParserListener) ExitCompound_sql_compiled(ctx *Compound_sql_compiledContext) {}

// EnterSql_statement_compiled is called when production sql_statement_compiled is entered.
func (s *BaseDb2ParserListener) EnterSql_statement_compiled(ctx *Sql_statement_compiledContext) {}

// ExitSql_statement_compiled is called when production sql_statement_compiled is exited.
func (s *BaseDb2ParserListener) ExitSql_statement_compiled(ctx *Sql_statement_compiledContext) {}

// EnterCreate_module_statement is called when production create_module_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_module_statement(ctx *Create_module_statementContext) {}

// ExitCreate_module_statement is called when production create_module_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_module_statement(ctx *Create_module_statementContext) {}

// EnterCreate_nickname_statement is called when production create_nickname_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_nickname_statement(ctx *Create_nickname_statementContext) {
}

// ExitCreate_nickname_statement is called when production create_nickname_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_nickname_statement(ctx *Create_nickname_statementContext) {
}

// EnterNick_name_option_name is called when production nick_name_option_name is entered.
func (s *BaseDb2ParserListener) EnterNick_name_option_name(ctx *Nick_name_option_nameContext) {}

// ExitNick_name_option_name is called when production nick_name_option_name is exited.
func (s *BaseDb2ParserListener) ExitNick_name_option_name(ctx *Nick_name_option_nameContext) {}

// EnterRemote_object_name is called when production remote_object_name is entered.
func (s *BaseDb2ParserListener) EnterRemote_object_name(ctx *Remote_object_nameContext) {}

// ExitRemote_object_name is called when production remote_object_name is exited.
func (s *BaseDb2ParserListener) ExitRemote_object_name(ctx *Remote_object_nameContext) {}

// EnterNon_relational_data_definition is called when production non_relational_data_definition is entered.
func (s *BaseDb2ParserListener) EnterNon_relational_data_definition(ctx *Non_relational_data_definitionContext) {
}

// ExitNon_relational_data_definition is called when production non_relational_data_definition is exited.
func (s *BaseDb2ParserListener) ExitNon_relational_data_definition(ctx *Non_relational_data_definitionContext) {
}

// EnterNick_name_column_list is called when production nick_name_column_list is entered.
func (s *BaseDb2ParserListener) EnterNick_name_column_list(ctx *Nick_name_column_listContext) {}

// ExitNick_name_column_list is called when production nick_name_column_list is exited.
func (s *BaseDb2ParserListener) ExitNick_name_column_list(ctx *Nick_name_column_listContext) {}

// EnterNick_name_column_list_item is called when production nick_name_column_list_item is entered.
func (s *BaseDb2ParserListener) EnterNick_name_column_list_item(ctx *Nick_name_column_list_itemContext) {
}

// ExitNick_name_column_list_item is called when production nick_name_column_list_item is exited.
func (s *BaseDb2ParserListener) ExitNick_name_column_list_item(ctx *Nick_name_column_list_itemContext) {
}

// EnterNick_name_column_definition is called when production nick_name_column_definition is entered.
func (s *BaseDb2ParserListener) EnterNick_name_column_definition(ctx *Nick_name_column_definitionContext) {
}

// ExitNick_name_column_definition is called when production nick_name_column_definition is exited.
func (s *BaseDb2ParserListener) ExitNick_name_column_definition(ctx *Nick_name_column_definitionContext) {
}

// EnterNick_name_column_options is called when production nick_name_column_options is entered.
func (s *BaseDb2ParserListener) EnterNick_name_column_options(ctx *Nick_name_column_optionsContext) {}

// ExitNick_name_column_options is called when production nick_name_column_options is exited.
func (s *BaseDb2ParserListener) ExitNick_name_column_options(ctx *Nick_name_column_optionsContext) {}

// EnterFederated_column_options is called when production federated_column_options is entered.
func (s *BaseDb2ParserListener) EnterFederated_column_options(ctx *Federated_column_optionsContext) {}

// ExitFederated_column_options is called when production federated_column_options is exited.
func (s *BaseDb2ParserListener) ExitFederated_column_options(ctx *Federated_column_optionsContext) {}

// EnterColumn_option_name is called when production column_option_name is entered.
func (s *BaseDb2ParserListener) EnterColumn_option_name(ctx *Column_option_nameContext) {}

// ExitColumn_option_name is called when production column_option_name is exited.
func (s *BaseDb2ParserListener) ExitColumn_option_name(ctx *Column_option_nameContext) {}

// EnterCreate_permission_statement is called when production create_permission_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_permission_statement(ctx *Create_permission_statementContext) {
}

// ExitCreate_permission_statement is called when production create_permission_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_permission_statement(ctx *Create_permission_statementContext) {
}

// EnterCreate_procedure_statement is called when production create_procedure_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_procedure_statement(ctx *Create_procedure_statementContext) {
}

// ExitCreate_procedure_statement is called when production create_procedure_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_procedure_statement(ctx *Create_procedure_statementContext) {
}

// EnterCreate_procedure_external_statement is called when production create_procedure_external_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_procedure_external_statement(ctx *Create_procedure_external_statementContext) {
}

// ExitCreate_procedure_external_statement is called when production create_procedure_external_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_procedure_external_statement(ctx *Create_procedure_external_statementContext) {
}

// EnterProc_ext_param_list is called when production proc_ext_param_list is entered.
func (s *BaseDb2ParserListener) EnterProc_ext_param_list(ctx *Proc_ext_param_listContext) {}

// ExitProc_ext_param_list is called when production proc_ext_param_list is exited.
func (s *BaseDb2ParserListener) ExitProc_ext_param_list(ctx *Proc_ext_param_listContext) {}

// EnterProc_ext_param is called when production proc_ext_param is entered.
func (s *BaseDb2ParserListener) EnterProc_ext_param(ctx *Proc_ext_paramContext) {}

// ExitProc_ext_param is called when production proc_ext_param is exited.
func (s *BaseDb2ParserListener) ExitProc_ext_param(ctx *Proc_ext_paramContext) {}

// EnterOption_list_2 is called when production option_list_2 is entered.
func (s *BaseDb2ParserListener) EnterOption_list_2(ctx *Option_list_2Context) {}

// ExitOption_list_2 is called when production option_list_2 is exited.
func (s *BaseDb2ParserListener) ExitOption_list_2(ctx *Option_list_2Context) {}

// EnterOption_list_2_item is called when production option_list_2_item is entered.
func (s *BaseDb2ParserListener) EnterOption_list_2_item(ctx *Option_list_2_itemContext) {}

// ExitOption_list_2_item is called when production option_list_2_item is exited.
func (s *BaseDb2ParserListener) ExitOption_list_2_item(ctx *Option_list_2_itemContext) {}

// EnterCreate_procedure_sourced_statement is called when production create_procedure_sourced_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_procedure_sourced_statement(ctx *Create_procedure_sourced_statementContext) {
}

// ExitCreate_procedure_sourced_statement is called when production create_procedure_sourced_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_procedure_sourced_statement(ctx *Create_procedure_sourced_statementContext) {
}

// EnterSource_procedure_clause is called when production source_procedure_clause is entered.
func (s *BaseDb2ParserListener) EnterSource_procedure_clause(ctx *Source_procedure_clauseContext) {}

// ExitSource_procedure_clause is called when production source_procedure_clause is exited.
func (s *BaseDb2ParserListener) ExitSource_procedure_clause(ctx *Source_procedure_clauseContext) {}

// EnterSource_object_name is called when production source_object_name is entered.
func (s *BaseDb2ParserListener) EnterSource_object_name(ctx *Source_object_nameContext) {}

// ExitSource_object_name is called when production source_object_name is exited.
func (s *BaseDb2ParserListener) ExitSource_object_name(ctx *Source_object_nameContext) {}

// EnterOption_list_1 is called when production option_list_1 is entered.
func (s *BaseDb2ParserListener) EnterOption_list_1(ctx *Option_list_1Context) {}

// ExitOption_list_1 is called when production option_list_1 is exited.
func (s *BaseDb2ParserListener) ExitOption_list_1(ctx *Option_list_1Context) {}

// EnterOption_list_1_item is called when production option_list_1_item is entered.
func (s *BaseDb2ParserListener) EnterOption_list_1_item(ctx *Option_list_1_itemContext) {}

// ExitOption_list_1_item is called when production option_list_1_item is exited.
func (s *BaseDb2ParserListener) ExitOption_list_1_item(ctx *Option_list_1_itemContext) {}

// EnterResult_set_element_number is called when production result_set_element_number is entered.
func (s *BaseDb2ParserListener) EnterResult_set_element_number(ctx *Result_set_element_numberContext) {
}

// ExitResult_set_element_number is called when production result_set_element_number is exited.
func (s *BaseDb2ParserListener) ExitResult_set_element_number(ctx *Result_set_element_numberContext) {
}

// EnterUnique_id is called when production unique_id is entered.
func (s *BaseDb2ParserListener) EnterUnique_id(ctx *Unique_idContext) {}

// ExitUnique_id is called when production unique_id is exited.
func (s *BaseDb2ParserListener) ExitUnique_id(ctx *Unique_idContext) {}

// EnterCreate_procedure_sql_statement is called when production create_procedure_sql_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_procedure_sql_statement(ctx *Create_procedure_sql_statementContext) {
}

// ExitCreate_procedure_sql_statement is called when production create_procedure_sql_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_procedure_sql_statement(ctx *Create_procedure_sql_statementContext) {
}

// EnterProc_parameter_list is called when production proc_parameter_list is entered.
func (s *BaseDb2ParserListener) EnterProc_parameter_list(ctx *Proc_parameter_listContext) {}

// ExitProc_parameter_list is called when production proc_parameter_list is exited.
func (s *BaseDb2ParserListener) ExitProc_parameter_list(ctx *Proc_parameter_listContext) {}

// EnterProc_parameter_list_item is called when production proc_parameter_list_item is entered.
func (s *BaseDb2ParserListener) EnterProc_parameter_list_item(ctx *Proc_parameter_list_itemContext) {}

// ExitProc_parameter_list_item is called when production proc_parameter_list_item is exited.
func (s *BaseDb2ParserListener) ExitProc_parameter_list_item(ctx *Proc_parameter_list_itemContext) {}

// EnterIn_out_inout is called when production in_out_inout is entered.
func (s *BaseDb2ParserListener) EnterIn_out_inout(ctx *In_out_inoutContext) {}

// ExitIn_out_inout is called when production in_out_inout is exited.
func (s *BaseDb2ParserListener) ExitIn_out_inout(ctx *In_out_inoutContext) {}

// EnterOption_list is called when production option_list is entered.
func (s *BaseDb2ParserListener) EnterOption_list(ctx *Option_listContext) {}

// ExitOption_list is called when production option_list is exited.
func (s *BaseDb2ParserListener) ExitOption_list(ctx *Option_listContext) {}

// EnterOption_list_item is called when production option_list_item is entered.
func (s *BaseDb2ParserListener) EnterOption_list_item(ctx *Option_list_itemContext) {}

// ExitOption_list_item is called when production option_list_item is exited.
func (s *BaseDb2ParserListener) ExitOption_list_item(ctx *Option_list_itemContext) {}

// EnterSql_procedure_body is called when production sql_procedure_body is entered.
func (s *BaseDb2ParserListener) EnterSql_procedure_body(ctx *Sql_procedure_bodyContext) {}

// ExitSql_procedure_body is called when production sql_procedure_body is exited.
func (s *BaseDb2ParserListener) ExitSql_procedure_body(ctx *Sql_procedure_bodyContext) {}

// EnterCreate_role_statement is called when production create_role_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_role_statement(ctx *Create_role_statementContext) {}

// ExitCreate_role_statement is called when production create_role_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_role_statement(ctx *Create_role_statementContext) {}

// EnterCreate_schema_statement is called when production create_schema_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_schema_statement(ctx *Create_schema_statementContext) {}

// ExitCreate_schema_statement is called when production create_schema_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_schema_statement(ctx *Create_schema_statementContext) {}

// EnterSchema_sql_statement is called when production schema_sql_statement is entered.
func (s *BaseDb2ParserListener) EnterSchema_sql_statement(ctx *Schema_sql_statementContext) {}

// ExitSchema_sql_statement is called when production schema_sql_statement is exited.
func (s *BaseDb2ParserListener) ExitSchema_sql_statement(ctx *Schema_sql_statementContext) {}

// EnterCreate_security_label_component_statement is called when production create_security_label_component_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_security_label_component_statement(ctx *Create_security_label_component_statementContext) {
}

// ExitCreate_security_label_component_statement is called when production create_security_label_component_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_security_label_component_statement(ctx *Create_security_label_component_statementContext) {
}

// EnterArray_clause is called when production array_clause is entered.
func (s *BaseDb2ParserListener) EnterArray_clause(ctx *Array_clauseContext) {}

// ExitArray_clause is called when production array_clause is exited.
func (s *BaseDb2ParserListener) ExitArray_clause(ctx *Array_clauseContext) {}

// EnterSet_clause is called when production set_clause is entered.
func (s *BaseDb2ParserListener) EnterSet_clause(ctx *Set_clauseContext) {}

// ExitSet_clause is called when production set_clause is exited.
func (s *BaseDb2ParserListener) ExitSet_clause(ctx *Set_clauseContext) {}

// EnterTree_clause is called when production tree_clause is entered.
func (s *BaseDb2ParserListener) EnterTree_clause(ctx *Tree_clauseContext) {}

// ExitTree_clause is called when production tree_clause is exited.
func (s *BaseDb2ParserListener) ExitTree_clause(ctx *Tree_clauseContext) {}

// EnterTree_clause_item is called when production tree_clause_item is entered.
func (s *BaseDb2ParserListener) EnterTree_clause_item(ctx *Tree_clause_itemContext) {}

// ExitTree_clause_item is called when production tree_clause_item is exited.
func (s *BaseDb2ParserListener) ExitTree_clause_item(ctx *Tree_clause_itemContext) {}

// EnterCreate_security_label_statement is called when production create_security_label_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_security_label_statement(ctx *Create_security_label_statementContext) {
}

// ExitCreate_security_label_statement is called when production create_security_label_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_security_label_statement(ctx *Create_security_label_statementContext) {
}

// EnterCreate_security_label_item is called when production create_security_label_item is entered.
func (s *BaseDb2ParserListener) EnterCreate_security_label_item(ctx *Create_security_label_itemContext) {
}

// ExitCreate_security_label_item is called when production create_security_label_item is exited.
func (s *BaseDb2ParserListener) ExitCreate_security_label_item(ctx *Create_security_label_itemContext) {
}

// EnterCreate_security_policy_statement is called when production create_security_policy_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_security_policy_statement(ctx *Create_security_policy_statementContext) {
}

// ExitCreate_security_policy_statement is called when production create_security_policy_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_security_policy_statement(ctx *Create_security_policy_statementContext) {
}

// EnterCreate_sequence_statement is called when production create_sequence_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_sequence_statement(ctx *Create_sequence_statementContext) {
}

// ExitCreate_sequence_statement is called when production create_sequence_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_sequence_statement(ctx *Create_sequence_statementContext) {
}

// EnterCreate_sequence_opts is called when production create_sequence_opts is entered.
func (s *BaseDb2ParserListener) EnterCreate_sequence_opts(ctx *Create_sequence_optsContext) {}

// ExitCreate_sequence_opts is called when production create_sequence_opts is exited.
func (s *BaseDb2ParserListener) ExitCreate_sequence_opts(ctx *Create_sequence_optsContext) {}

// EnterCreate_sequence_opts_item is called when production create_sequence_opts_item is entered.
func (s *BaseDb2ParserListener) EnterCreate_sequence_opts_item(ctx *Create_sequence_opts_itemContext) {
}

// ExitCreate_sequence_opts_item is called when production create_sequence_opts_item is exited.
func (s *BaseDb2ParserListener) ExitCreate_sequence_opts_item(ctx *Create_sequence_opts_itemContext) {
}

// EnterCreate_service_class_statement is called when production create_service_class_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_service_class_statement(ctx *Create_service_class_statementContext) {
}

// ExitCreate_service_class_statement is called when production create_service_class_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_service_class_statement(ctx *Create_service_class_statementContext) {
}

// EnterHigh_medium_low is called when production high_medium_low is entered.
func (s *BaseDb2ParserListener) EnterHigh_medium_low(ctx *High_medium_lowContext) {}

// ExitHigh_medium_low is called when production high_medium_low is exited.
func (s *BaseDb2ParserListener) ExitHigh_medium_low(ctx *High_medium_lowContext) {}

// EnterOn_off is called when production on_off is entered.
func (s *BaseDb2ParserListener) EnterOn_off(ctx *On_offContext) {}

// ExitOn_off is called when production on_off is exited.
func (s *BaseDb2ParserListener) ExitOn_off(ctx *On_offContext) {}

// EnterSoft_hard is called when production soft_hard is entered.
func (s *BaseDb2ParserListener) EnterSoft_hard(ctx *Soft_hardContext) {}

// ExitSoft_hard is called when production soft_hard is exited.
func (s *BaseDb2ParserListener) ExitSoft_hard(ctx *Soft_hardContext) {}

// EnterCreate_server_statement is called when production create_server_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_server_statement(ctx *Create_server_statementContext) {}

// ExitCreate_server_statement is called when production create_server_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_server_statement(ctx *Create_server_statementContext) {}

// EnterPassword_ is called when production password_ is entered.
func (s *BaseDb2ParserListener) EnterPassword_(ctx *Password_Context) {}

// ExitPassword_ is called when production password_ is exited.
func (s *BaseDb2ParserListener) ExitPassword_(ctx *Password_Context) {}

// EnterCreate_stogroup_statement is called when production create_stogroup_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_stogroup_statement(ctx *Create_stogroup_statementContext) {
}

// ExitCreate_stogroup_statement is called when production create_stogroup_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_stogroup_statement(ctx *Create_stogroup_statementContext) {
}

// EnterCreate_stogroup_opts is called when production create_stogroup_opts is entered.
func (s *BaseDb2ParserListener) EnterCreate_stogroup_opts(ctx *Create_stogroup_optsContext) {}

// ExitCreate_stogroup_opts is called when production create_stogroup_opts is exited.
func (s *BaseDb2ParserListener) ExitCreate_stogroup_opts(ctx *Create_stogroup_optsContext) {}

// EnterCreate_synonym_statement is called when production create_synonym_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_synonym_statement(ctx *Create_synonym_statementContext) {}

// ExitCreate_synonym_statement is called when production create_synonym_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_synonym_statement(ctx *Create_synonym_statementContext) {}

// EnterCreate_table_statement is called when production create_table_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_table_statement(ctx *Create_table_statementContext) {}

// ExitCreate_table_statement is called when production create_table_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_table_statement(ctx *Create_table_statementContext) {}

// EnterCreate_table_opts is called when production create_table_opts is entered.
func (s *BaseDb2ParserListener) EnterCreate_table_opts(ctx *Create_table_optsContext) {}

// ExitCreate_table_opts is called when production create_table_opts is exited.
func (s *BaseDb2ParserListener) ExitCreate_table_opts(ctx *Create_table_optsContext) {}

// EnterTable_option_list is called when production table_option_list is entered.
func (s *BaseDb2ParserListener) EnterTable_option_list(ctx *Table_option_listContext) {}

// ExitTable_option_list is called when production table_option_list is exited.
func (s *BaseDb2ParserListener) ExitTable_option_list(ctx *Table_option_listContext) {}

// EnterTable_option_list_item is called when production table_option_list_item is entered.
func (s *BaseDb2ParserListener) EnterTable_option_list_item(ctx *Table_option_list_itemContext) {}

// ExitTable_option_list_item is called when production table_option_list_item is exited.
func (s *BaseDb2ParserListener) ExitTable_option_list_item(ctx *Table_option_list_itemContext) {}

// EnterTable_option_name is called when production table_option_name is entered.
func (s *BaseDb2ParserListener) EnterTable_option_name(ctx *Table_option_nameContext) {}

// ExitTable_option_name is called when production table_option_name is exited.
func (s *BaseDb2ParserListener) ExitTable_option_name(ctx *Table_option_nameContext) {}

// EnterElement_list is called when production element_list is entered.
func (s *BaseDb2ParserListener) EnterElement_list(ctx *Element_listContext) {}

// ExitElement_list is called when production element_list is exited.
func (s *BaseDb2ParserListener) ExitElement_list(ctx *Element_listContext) {}

// EnterElement_list_item is called when production element_list_item is entered.
func (s *BaseDb2ParserListener) EnterElement_list_item(ctx *Element_list_itemContext) {}

// ExitElement_list_item is called when production element_list_item is exited.
func (s *BaseDb2ParserListener) ExitElement_list_item(ctx *Element_list_itemContext) {}

// EnterColumn_definition is called when production column_definition is entered.
func (s *BaseDb2ParserListener) EnterColumn_definition(ctx *Column_definitionContext) {}

// ExitColumn_definition is called when production column_definition is exited.
func (s *BaseDb2ParserListener) ExitColumn_definition(ctx *Column_definitionContext) {}

// EnterPeriod_definition is called when production period_definition is entered.
func (s *BaseDb2ParserListener) EnterPeriod_definition(ctx *Period_definitionContext) {}

// ExitPeriod_definition is called when production period_definition is exited.
func (s *BaseDb2ParserListener) ExitPeriod_definition(ctx *Period_definitionContext) {}

// EnterUnique_constraint is called when production unique_constraint is entered.
func (s *BaseDb2ParserListener) EnterUnique_constraint(ctx *Unique_constraintContext) {}

// ExitUnique_constraint is called when production unique_constraint is exited.
func (s *BaseDb2ParserListener) ExitUnique_constraint(ctx *Unique_constraintContext) {}

// EnterReferential_constraint is called when production referential_constraint is entered.
func (s *BaseDb2ParserListener) EnterReferential_constraint(ctx *Referential_constraintContext) {}

// ExitReferential_constraint is called when production referential_constraint is exited.
func (s *BaseDb2ParserListener) ExitReferential_constraint(ctx *Referential_constraintContext) {}

// EnterCheck_constraint is called when production check_constraint is entered.
func (s *BaseDb2ParserListener) EnterCheck_constraint(ctx *Check_constraintContext) {}

// ExitCheck_constraint is called when production check_constraint is exited.
func (s *BaseDb2ParserListener) ExitCheck_constraint(ctx *Check_constraintContext) {}

// EnterColumn_options is called when production column_options is entered.
func (s *BaseDb2ParserListener) EnterColumn_options(ctx *Column_optionsContext) {}

// ExitColumn_options is called when production column_options is exited.
func (s *BaseDb2ParserListener) ExitColumn_options(ctx *Column_optionsContext) {}

// EnterColumn_options_item is called when production column_options_item is entered.
func (s *BaseDb2ParserListener) EnterColumn_options_item(ctx *Column_options_itemContext) {}

// ExitColumn_options_item is called when production column_options_item is exited.
func (s *BaseDb2ParserListener) ExitColumn_options_item(ctx *Column_options_itemContext) {}

// EnterReferences_clause is called when production references_clause is entered.
func (s *BaseDb2ParserListener) EnterReferences_clause(ctx *References_clauseContext) {}

// ExitReferences_clause is called when production references_clause is exited.
func (s *BaseDb2ParserListener) ExitReferences_clause(ctx *References_clauseContext) {}

// EnterRule_clause is called when production rule_clause is entered.
func (s *BaseDb2ParserListener) EnterRule_clause(ctx *Rule_clauseContext) {}

// ExitRule_clause is called when production rule_clause is exited.
func (s *BaseDb2ParserListener) ExitRule_clause(ctx *Rule_clauseContext) {}

// EnterConstraint_attributes is called when production constraint_attributes is entered.
func (s *BaseDb2ParserListener) EnterConstraint_attributes(ctx *Constraint_attributesContext) {}

// ExitConstraint_attributes is called when production constraint_attributes is exited.
func (s *BaseDb2ParserListener) ExitConstraint_attributes(ctx *Constraint_attributesContext) {}

// EnterDefault_clause is called when production default_clause is entered.
func (s *BaseDb2ParserListener) EnterDefault_clause(ctx *Default_clauseContext) {}

// ExitDefault_clause is called when production default_clause is exited.
func (s *BaseDb2ParserListener) ExitDefault_clause(ctx *Default_clauseContext) {}

// EnterDefault_values is called when production default_values is entered.
func (s *BaseDb2ParserListener) EnterDefault_values(ctx *Default_valuesContext) {}

// ExitDefault_values is called when production default_values is exited.
func (s *BaseDb2ParserListener) ExitDefault_values(ctx *Default_valuesContext) {}

// EnterGenerated_clause is called when production generated_clause is entered.
func (s *BaseDb2ParserListener) EnterGenerated_clause(ctx *Generated_clauseContext) {}

// ExitGenerated_clause is called when production generated_clause is exited.
func (s *BaseDb2ParserListener) ExitGenerated_clause(ctx *Generated_clauseContext) {}

// EnterDatetime_special_register is called when production datetime_special_register is entered.
func (s *BaseDb2ParserListener) EnterDatetime_special_register(ctx *Datetime_special_registerContext) {
}

// ExitDatetime_special_register is called when production datetime_special_register is exited.
func (s *BaseDb2ParserListener) ExitDatetime_special_register(ctx *Datetime_special_registerContext) {
}

// EnterUser_special_register is called when production user_special_register is entered.
func (s *BaseDb2ParserListener) EnterUser_special_register(ctx *User_special_registerContext) {}

// ExitUser_special_register is called when production user_special_register is exited.
func (s *BaseDb2ParserListener) ExitUser_special_register(ctx *User_special_registerContext) {}

// EnterCast_function is called when production cast_function is entered.
func (s *BaseDb2ParserListener) EnterCast_function(ctx *Cast_functionContext) {}

// ExitCast_function is called when production cast_function is exited.
func (s *BaseDb2ParserListener) ExitCast_function(ctx *Cast_functionContext) {}

// EnterIdentity_options is called when production identity_options is entered.
func (s *BaseDb2ParserListener) EnterIdentity_options(ctx *Identity_optionsContext) {}

// ExitIdentity_options is called when production identity_options is exited.
func (s *BaseDb2ParserListener) ExitIdentity_options(ctx *Identity_optionsContext) {}

// EnterIdentity_options_item is called when production identity_options_item is entered.
func (s *BaseDb2ParserListener) EnterIdentity_options_item(ctx *Identity_options_itemContext) {}

// ExitIdentity_options_item is called when production identity_options_item is exited.
func (s *BaseDb2ParserListener) ExitIdentity_options_item(ctx *Identity_options_itemContext) {}

// EnterAs_row_change_timestamp_clause is called when production as_row_change_timestamp_clause is entered.
func (s *BaseDb2ParserListener) EnterAs_row_change_timestamp_clause(ctx *As_row_change_timestamp_clauseContext) {
}

// ExitAs_row_change_timestamp_clause is called when production as_row_change_timestamp_clause is exited.
func (s *BaseDb2ParserListener) ExitAs_row_change_timestamp_clause(ctx *As_row_change_timestamp_clauseContext) {
}

// EnterAs_generated_expression_clause is called when production as_generated_expression_clause is entered.
func (s *BaseDb2ParserListener) EnterAs_generated_expression_clause(ctx *As_generated_expression_clauseContext) {
}

// ExitAs_generated_expression_clause is called when production as_generated_expression_clause is exited.
func (s *BaseDb2ParserListener) ExitAs_generated_expression_clause(ctx *As_generated_expression_clauseContext) {
}

// EnterGeneration_expression is called when production generation_expression is entered.
func (s *BaseDb2ParserListener) EnterGeneration_expression(ctx *Generation_expressionContext) {}

// ExitGeneration_expression is called when production generation_expression is exited.
func (s *BaseDb2ParserListener) ExitGeneration_expression(ctx *Generation_expressionContext) {}

// EnterAs_row_transaction_timestamp_clause is called when production as_row_transaction_timestamp_clause is entered.
func (s *BaseDb2ParserListener) EnterAs_row_transaction_timestamp_clause(ctx *As_row_transaction_timestamp_clauseContext) {
}

// ExitAs_row_transaction_timestamp_clause is called when production as_row_transaction_timestamp_clause is exited.
func (s *BaseDb2ParserListener) ExitAs_row_transaction_timestamp_clause(ctx *As_row_transaction_timestamp_clauseContext) {
}

// EnterAs_row_transaction_start_id_clause is called when production as_row_transaction_start_id_clause is entered.
func (s *BaseDb2ParserListener) EnterAs_row_transaction_start_id_clause(ctx *As_row_transaction_start_id_clauseContext) {
}

// ExitAs_row_transaction_start_id_clause is called when production as_row_transaction_start_id_clause is exited.
func (s *BaseDb2ParserListener) ExitAs_row_transaction_start_id_clause(ctx *As_row_transaction_start_id_clauseContext) {
}

// EnterOid_column_definition is called when production oid_column_definition is entered.
func (s *BaseDb2ParserListener) EnterOid_column_definition(ctx *Oid_column_definitionContext) {}

// ExitOid_column_definition is called when production oid_column_definition is exited.
func (s *BaseDb2ParserListener) ExitOid_column_definition(ctx *Oid_column_definitionContext) {}

// EnterRange_partition_spec is called when production range_partition_spec is entered.
func (s *BaseDb2ParserListener) EnterRange_partition_spec(ctx *Range_partition_specContext) {}

// ExitRange_partition_spec is called when production range_partition_spec is exited.
func (s *BaseDb2ParserListener) ExitRange_partition_spec(ctx *Range_partition_specContext) {}

// EnterPartition_expression_list is called when production partition_expression_list is entered.
func (s *BaseDb2ParserListener) EnterPartition_expression_list(ctx *Partition_expression_listContext) {
}

// ExitPartition_expression_list is called when production partition_expression_list is exited.
func (s *BaseDb2ParserListener) ExitPartition_expression_list(ctx *Partition_expression_listContext) {
}

// EnterPartition_expression is called when production partition_expression is entered.
func (s *BaseDb2ParserListener) EnterPartition_expression(ctx *Partition_expressionContext) {}

// ExitPartition_expression is called when production partition_expression is exited.
func (s *BaseDb2ParserListener) ExitPartition_expression(ctx *Partition_expressionContext) {}

// EnterPartition_element_list is called when production partition_element_list is entered.
func (s *BaseDb2ParserListener) EnterPartition_element_list(ctx *Partition_element_listContext) {}

// ExitPartition_element_list is called when production partition_element_list is exited.
func (s *BaseDb2ParserListener) ExitPartition_element_list(ctx *Partition_element_listContext) {}

// EnterPartition_element is called when production partition_element is entered.
func (s *BaseDb2ParserListener) EnterPartition_element(ctx *Partition_elementContext) {}

// ExitPartition_element is called when production partition_element is exited.
func (s *BaseDb2ParserListener) ExitPartition_element(ctx *Partition_elementContext) {}

// EnterBoundary_spec is called when production boundary_spec is entered.
func (s *BaseDb2ParserListener) EnterBoundary_spec(ctx *Boundary_specContext) {}

// ExitBoundary_spec is called when production boundary_spec is exited.
func (s *BaseDb2ParserListener) ExitBoundary_spec(ctx *Boundary_specContext) {}

// EnterPartition_tablespace_options is called when production partition_tablespace_options is entered.
func (s *BaseDb2ParserListener) EnterPartition_tablespace_options(ctx *Partition_tablespace_optionsContext) {
}

// ExitPartition_tablespace_options is called when production partition_tablespace_options is exited.
func (s *BaseDb2ParserListener) ExitPartition_tablespace_options(ctx *Partition_tablespace_optionsContext) {
}

// EnterDuration_label is called when production duration_label is entered.
func (s *BaseDb2ParserListener) EnterDuration_label(ctx *Duration_labelContext) {}

// ExitDuration_label is called when production duration_label is exited.
func (s *BaseDb2ParserListener) ExitDuration_label(ctx *Duration_labelContext) {}

// EnterStarting_clause is called when production starting_clause is entered.
func (s *BaseDb2ParserListener) EnterStarting_clause(ctx *Starting_clauseContext) {}

// ExitStarting_clause is called when production starting_clause is exited.
func (s *BaseDb2ParserListener) ExitStarting_clause(ctx *Starting_clauseContext) {}

// EnterConst_min_max_list is called when production const_min_max_list is entered.
func (s *BaseDb2ParserListener) EnterConst_min_max_list(ctx *Const_min_max_listContext) {}

// ExitConst_min_max_list is called when production const_min_max_list is exited.
func (s *BaseDb2ParserListener) ExitConst_min_max_list(ctx *Const_min_max_listContext) {}

// EnterConst_min_max is called when production const_min_max is entered.
func (s *BaseDb2ParserListener) EnterConst_min_max(ctx *Const_min_maxContext) {}

// ExitConst_min_max is called when production const_min_max is exited.
func (s *BaseDb2ParserListener) ExitConst_min_max(ctx *Const_min_maxContext) {}

// EnterEnding_clause is called when production ending_clause is entered.
func (s *BaseDb2ParserListener) EnterEnding_clause(ctx *Ending_clauseContext) {}

// ExitEnding_clause is called when production ending_clause is exited.
func (s *BaseDb2ParserListener) ExitEnding_clause(ctx *Ending_clauseContext) {}

// EnterTyped_table_options is called when production typed_table_options is entered.
func (s *BaseDb2ParserListener) EnterTyped_table_options(ctx *Typed_table_optionsContext) {}

// ExitTyped_table_options is called when production typed_table_options is exited.
func (s *BaseDb2ParserListener) ExitTyped_table_options(ctx *Typed_table_optionsContext) {}

// EnterTyped_element_list is called when production typed_element_list is entered.
func (s *BaseDb2ParserListener) EnterTyped_element_list(ctx *Typed_element_listContext) {}

// ExitTyped_element_list is called when production typed_element_list is exited.
func (s *BaseDb2ParserListener) ExitTyped_element_list(ctx *Typed_element_listContext) {}

// EnterTyped_element_list_item is called when production typed_element_list_item is entered.
func (s *BaseDb2ParserListener) EnterTyped_element_list_item(ctx *Typed_element_list_itemContext) {}

// ExitTyped_element_list_item is called when production typed_element_list_item is exited.
func (s *BaseDb2ParserListener) ExitTyped_element_list_item(ctx *Typed_element_list_itemContext) {}

// EnterAs_result_table is called when production as_result_table is entered.
func (s *BaseDb2ParserListener) EnterAs_result_table(ctx *As_result_tableContext) {}

// ExitAs_result_table is called when production as_result_table is exited.
func (s *BaseDb2ParserListener) ExitAs_result_table(ctx *As_result_tableContext) {}

// EnterCopy_options is called when production copy_options is entered.
func (s *BaseDb2ParserListener) EnterCopy_options(ctx *Copy_optionsContext) {}

// ExitCopy_options is called when production copy_options is exited.
func (s *BaseDb2ParserListener) ExitCopy_options(ctx *Copy_optionsContext) {}

// EnterMaterialized_query_options is called when production materialized_query_options is entered.
func (s *BaseDb2ParserListener) EnterMaterialized_query_options(ctx *Materialized_query_optionsContext) {
}

// ExitMaterialized_query_options is called when production materialized_query_options is exited.
func (s *BaseDb2ParserListener) ExitMaterialized_query_options(ctx *Materialized_query_optionsContext) {
}

// EnterStaging_table_definition is called when production staging_table_definition is entered.
func (s *BaseDb2ParserListener) EnterStaging_table_definition(ctx *Staging_table_definitionContext) {}

// ExitStaging_table_definition is called when production staging_table_definition is exited.
func (s *BaseDb2ParserListener) ExitStaging_table_definition(ctx *Staging_table_definitionContext) {}

// EnterDimensions_clause is called when production dimensions_clause is entered.
func (s *BaseDb2ParserListener) EnterDimensions_clause(ctx *Dimensions_clauseContext) {}

// ExitDimensions_clause is called when production dimensions_clause is exited.
func (s *BaseDb2ParserListener) ExitDimensions_clause(ctx *Dimensions_clauseContext) {}

// EnterCol_names is called when production col_names is entered.
func (s *BaseDb2ParserListener) EnterCol_names(ctx *Col_namesContext) {}

// ExitCol_names is called when production col_names is exited.
func (s *BaseDb2ParserListener) ExitCol_names(ctx *Col_namesContext) {}

// EnterSequence_key_spec is called when production sequence_key_spec is entered.
func (s *BaseDb2ParserListener) EnterSequence_key_spec(ctx *Sequence_key_specContext) {}

// ExitSequence_key_spec is called when production sequence_key_spec is exited.
func (s *BaseDb2ParserListener) ExitSequence_key_spec(ctx *Sequence_key_specContext) {}

// EnterSequence_key_spec_list is called when production sequence_key_spec_list is entered.
func (s *BaseDb2ParserListener) EnterSequence_key_spec_list(ctx *Sequence_key_spec_listContext) {}

// ExitSequence_key_spec_list is called when production sequence_key_spec_list is exited.
func (s *BaseDb2ParserListener) ExitSequence_key_spec_list(ctx *Sequence_key_spec_listContext) {}

// EnterSequence_key_spec_list_item is called when production sequence_key_spec_list_item is entered.
func (s *BaseDb2ParserListener) EnterSequence_key_spec_list_item(ctx *Sequence_key_spec_list_itemContext) {
}

// ExitSequence_key_spec_list_item is called when production sequence_key_spec_list_item is exited.
func (s *BaseDb2ParserListener) ExitSequence_key_spec_list_item(ctx *Sequence_key_spec_list_itemContext) {
}

// EnterTablespace_clauses is called when production tablespace_clauses is entered.
func (s *BaseDb2ParserListener) EnterTablespace_clauses(ctx *Tablespace_clausesContext) {}

// ExitTablespace_clauses is called when production tablespace_clauses is exited.
func (s *BaseDb2ParserListener) ExitTablespace_clauses(ctx *Tablespace_clausesContext) {}

// EnterDistribution_clause is called when production distribution_clause is entered.
func (s *BaseDb2ParserListener) EnterDistribution_clause(ctx *Distribution_clauseContext) {}

// ExitDistribution_clause is called when production distribution_clause is exited.
func (s *BaseDb2ParserListener) ExitDistribution_clause(ctx *Distribution_clauseContext) {}

// EnterPartitioning_clause is called when production partitioning_clause is entered.
func (s *BaseDb2ParserListener) EnterPartitioning_clause(ctx *Partitioning_clauseContext) {}

// ExitPartitioning_clause is called when production partitioning_clause is exited.
func (s *BaseDb2ParserListener) ExitPartitioning_clause(ctx *Partitioning_clauseContext) {}

// EnterIf_not_exists is called when production if_not_exists is entered.
func (s *BaseDb2ParserListener) EnterIf_not_exists(ctx *If_not_existsContext) {}

// ExitIf_not_exists is called when production if_not_exists is exited.
func (s *BaseDb2ParserListener) ExitIf_not_exists(ctx *If_not_existsContext) {}

// EnterCreate_tablespace_statement is called when production create_tablespace_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_tablespace_statement(ctx *Create_tablespace_statementContext) {
}

// ExitCreate_tablespace_statement is called when production create_tablespace_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_tablespace_statement(ctx *Create_tablespace_statementContext) {
}

// EnterStorage_group is called when production storage_group is entered.
func (s *BaseDb2ParserListener) EnterStorage_group(ctx *Storage_groupContext) {}

// ExitStorage_group is called when production storage_group is exited.
func (s *BaseDb2ParserListener) ExitStorage_group(ctx *Storage_groupContext) {}

// EnterSize_attributes is called when production size_attributes is entered.
func (s *BaseDb2ParserListener) EnterSize_attributes(ctx *Size_attributesContext) {}

// ExitSize_attributes is called when production size_attributes is exited.
func (s *BaseDb2ParserListener) ExitSize_attributes(ctx *Size_attributesContext) {}

// EnterSystem_containers is called when production system_containers is entered.
func (s *BaseDb2ParserListener) EnterSystem_containers(ctx *System_containersContext) {}

// ExitSystem_containers is called when production system_containers is exited.
func (s *BaseDb2ParserListener) ExitSystem_containers(ctx *System_containersContext) {}

// EnterContainer_string_list is called when production container_string_list is entered.
func (s *BaseDb2ParserListener) EnterContainer_string_list(ctx *Container_string_listContext) {}

// ExitContainer_string_list is called when production container_string_list is exited.
func (s *BaseDb2ParserListener) ExitContainer_string_list(ctx *Container_string_listContext) {}

// EnterDatabase_containers is called when production database_containers is entered.
func (s *BaseDb2ParserListener) EnterDatabase_containers(ctx *Database_containersContext) {}

// ExitDatabase_containers is called when production database_containers is exited.
func (s *BaseDb2ParserListener) ExitDatabase_containers(ctx *Database_containersContext) {}

// EnterContainer_clause is called when production container_clause is entered.
func (s *BaseDb2ParserListener) EnterContainer_clause(ctx *Container_clauseContext) {}

// ExitContainer_clause is called when production container_clause is exited.
func (s *BaseDb2ParserListener) ExitContainer_clause(ctx *Container_clauseContext) {}

// EnterContainer_clause_list is called when production container_clause_list is entered.
func (s *BaseDb2ParserListener) EnterContainer_clause_list(ctx *Container_clause_listContext) {}

// ExitContainer_clause_list is called when production container_clause_list is exited.
func (s *BaseDb2ParserListener) ExitContainer_clause_list(ctx *Container_clause_listContext) {}

// EnterContainer_clause_list_item is called when production container_clause_list_item is entered.
func (s *BaseDb2ParserListener) EnterContainer_clause_list_item(ctx *Container_clause_list_itemContext) {
}

// ExitContainer_clause_list_item is called when production container_clause_list_item is exited.
func (s *BaseDb2ParserListener) ExitContainer_clause_list_item(ctx *Container_clause_list_itemContext) {
}

// EnterOn_db_partitions_clause is called when production on_db_partitions_clause is entered.
func (s *BaseDb2ParserListener) EnterOn_db_partitions_clause(ctx *On_db_partitions_clauseContext) {}

// ExitOn_db_partitions_clause is called when production on_db_partitions_clause is exited.
func (s *BaseDb2ParserListener) ExitOn_db_partitions_clause(ctx *On_db_partitions_clauseContext) {}

// EnterDb_partition_number_list is called when production db_partition_number_list is entered.
func (s *BaseDb2ParserListener) EnterDb_partition_number_list(ctx *Db_partition_number_listContext) {}

// ExitDb_partition_number_list is called when production db_partition_number_list is exited.
func (s *BaseDb2ParserListener) ExitDb_partition_number_list(ctx *Db_partition_number_listContext) {}

// EnterDb_partition_number_list_item is called when production db_partition_number_list_item is entered.
func (s *BaseDb2ParserListener) EnterDb_partition_number_list_item(ctx *Db_partition_number_list_itemContext) {
}

// ExitDb_partition_number_list_item is called when production db_partition_number_list_item is exited.
func (s *BaseDb2ParserListener) ExitDb_partition_number_list_item(ctx *Db_partition_number_list_itemContext) {
}

// EnterDb_partition_number is called when production db_partition_number is entered.
func (s *BaseDb2ParserListener) EnterDb_partition_number(ctx *Db_partition_numberContext) {}

// ExitDb_partition_number is called when production db_partition_number is exited.
func (s *BaseDb2ParserListener) ExitDb_partition_number(ctx *Db_partition_numberContext) {}

// EnterNumber_of_pages is called when production number_of_pages is entered.
func (s *BaseDb2ParserListener) EnterNumber_of_pages(ctx *Number_of_pagesContext) {}

// ExitNumber_of_pages is called when production number_of_pages is exited.
func (s *BaseDb2ParserListener) ExitNumber_of_pages(ctx *Number_of_pagesContext) {}

// EnterNumber_of_files is called when production number_of_files is entered.
func (s *BaseDb2ParserListener) EnterNumber_of_files(ctx *Number_of_filesContext) {}

// ExitNumber_of_files is called when production number_of_files is exited.
func (s *BaseDb2ParserListener) ExitNumber_of_files(ctx *Number_of_filesContext) {}

// EnterNumber_of_milliseconds is called when production number_of_milliseconds is entered.
func (s *BaseDb2ParserListener) EnterNumber_of_milliseconds(ctx *Number_of_millisecondsContext) {}

// ExitNumber_of_milliseconds is called when production number_of_milliseconds is exited.
func (s *BaseDb2ParserListener) ExitNumber_of_milliseconds(ctx *Number_of_millisecondsContext) {}

// EnterNumber_megabytes_per_second is called when production number_megabytes_per_second is entered.
func (s *BaseDb2ParserListener) EnterNumber_megabytes_per_second(ctx *Number_megabytes_per_secondContext) {
}

// ExitNumber_megabytes_per_second is called when production number_megabytes_per_second is exited.
func (s *BaseDb2ParserListener) ExitNumber_megabytes_per_second(ctx *Number_megabytes_per_secondContext) {
}

// EnterCreate_threshold_statement is called when production create_threshold_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_threshold_statement(ctx *Create_threshold_statementContext) {
}

// ExitCreate_threshold_statement is called when production create_threshold_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_threshold_statement(ctx *Create_threshold_statementContext) {
}

// EnterThreshold_domain is called when production threshold_domain is entered.
func (s *BaseDb2ParserListener) EnterThreshold_domain(ctx *Threshold_domainContext) {}

// ExitThreshold_domain is called when production threshold_domain is exited.
func (s *BaseDb2ParserListener) ExitThreshold_domain(ctx *Threshold_domainContext) {}

// EnterStatement_text is called when production statement_text is entered.
func (s *BaseDb2ParserListener) EnterStatement_text(ctx *Statement_textContext) {}

// ExitStatement_text is called when production statement_text is exited.
func (s *BaseDb2ParserListener) ExitStatement_text(ctx *Statement_textContext) {}

// EnterExecutable_id is called when production executable_id is entered.
func (s *BaseDb2ParserListener) EnterExecutable_id(ctx *Executable_idContext) {}

// ExitExecutable_id is called when production executable_id is exited.
func (s *BaseDb2ParserListener) ExitExecutable_id(ctx *Executable_idContext) {}

// EnterEnforcement_scope is called when production enforcement_scope is entered.
func (s *BaseDb2ParserListener) EnterEnforcement_scope(ctx *Enforcement_scopeContext) {}

// ExitEnforcement_scope is called when production enforcement_scope is exited.
func (s *BaseDb2ParserListener) ExitEnforcement_scope(ctx *Enforcement_scopeContext) {}

// EnterThreshold_predicate is called when production threshold_predicate is entered.
func (s *BaseDb2ParserListener) EnterThreshold_predicate(ctx *Threshold_predicateContext) {}

// ExitThreshold_predicate is called when production threshold_predicate is exited.
func (s *BaseDb2ParserListener) ExitThreshold_predicate(ctx *Threshold_predicateContext) {}

// EnterChecking_every is called when production checking_every is entered.
func (s *BaseDb2ParserListener) EnterChecking_every(ctx *Checking_everyContext) {}

// ExitChecking_every is called when production checking_every is exited.
func (s *BaseDb2ParserListener) ExitChecking_every(ctx *Checking_everyContext) {}

// EnterHour_to_seconds is called when production hour_to_seconds is entered.
func (s *BaseDb2ParserListener) EnterHour_to_seconds(ctx *Hour_to_secondsContext) {}

// ExitHour_to_seconds is called when production hour_to_seconds is exited.
func (s *BaseDb2ParserListener) ExitHour_to_seconds(ctx *Hour_to_secondsContext) {}

// EnterDay_to_minutes is called when production day_to_minutes is entered.
func (s *BaseDb2ParserListener) EnterDay_to_minutes(ctx *Day_to_minutesContext) {}

// ExitDay_to_minutes is called when production day_to_minutes is exited.
func (s *BaseDb2ParserListener) ExitDay_to_minutes(ctx *Day_to_minutesContext) {}

// EnterDay_to_seconds is called when production day_to_seconds is entered.
func (s *BaseDb2ParserListener) EnterDay_to_seconds(ctx *Day_to_secondsContext) {}

// ExitDay_to_seconds is called when production day_to_seconds is exited.
func (s *BaseDb2ParserListener) ExitDay_to_seconds(ctx *Day_to_secondsContext) {}

// EnterThreshold_exceeded_actions_2 is called when production threshold_exceeded_actions_2 is entered.
func (s *BaseDb2ParserListener) EnterThreshold_exceeded_actions_2(ctx *Threshold_exceeded_actions_2Context) {
}

// ExitThreshold_exceeded_actions_2 is called when production threshold_exceeded_actions_2 is exited.
func (s *BaseDb2ParserListener) ExitThreshold_exceeded_actions_2(ctx *Threshold_exceeded_actions_2Context) {
}

// EnterDetails_section is called when production details_section is entered.
func (s *BaseDb2ParserListener) EnterDetails_section(ctx *Details_sectionContext) {}

// ExitDetails_section is called when production details_section is exited.
func (s *BaseDb2ParserListener) ExitDetails_section(ctx *Details_sectionContext) {}

// EnterRemap_activity_action is called when production remap_activity_action is entered.
func (s *BaseDb2ParserListener) EnterRemap_activity_action(ctx *Remap_activity_actionContext) {}

// ExitRemap_activity_action is called when production remap_activity_action is exited.
func (s *BaseDb2ParserListener) ExitRemap_activity_action(ctx *Remap_activity_actionContext) {}

// EnterCreate_transform_statement is called when production create_transform_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_transform_statement(ctx *Create_transform_statementContext) {
}

// ExitCreate_transform_statement is called when production create_transform_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_transform_statement(ctx *Create_transform_statementContext) {
}

// EnterTranform_list is called when production tranform_list is entered.
func (s *BaseDb2ParserListener) EnterTranform_list(ctx *Tranform_listContext) {}

// ExitTranform_list is called when production tranform_list is exited.
func (s *BaseDb2ParserListener) ExitTranform_list(ctx *Tranform_listContext) {}

// EnterTranform_list_item is called when production tranform_list_item is entered.
func (s *BaseDb2ParserListener) EnterTranform_list_item(ctx *Tranform_list_itemContext) {}

// ExitTranform_list_item is called when production tranform_list_item is exited.
func (s *BaseDb2ParserListener) ExitTranform_list_item(ctx *Tranform_list_itemContext) {}

// EnterTransform_group_list is called when production transform_group_list is entered.
func (s *BaseDb2ParserListener) EnterTransform_group_list(ctx *Transform_group_listContext) {}

// ExitTransform_group_list is called when production transform_group_list is exited.
func (s *BaseDb2ParserListener) ExitTransform_group_list(ctx *Transform_group_listContext) {}

// EnterTransform_group_list_item is called when production transform_group_list_item is entered.
func (s *BaseDb2ParserListener) EnterTransform_group_list_item(ctx *Transform_group_list_itemContext) {
}

// ExitTransform_group_list_item is called when production transform_group_list_item is exited.
func (s *BaseDb2ParserListener) ExitTransform_group_list_item(ctx *Transform_group_list_itemContext) {
}

// EnterCreate_trigger_statement is called when production create_trigger_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_trigger_statement(ctx *Create_trigger_statementContext) {}

// ExitCreate_trigger_statement is called when production create_trigger_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_trigger_statement(ctx *Create_trigger_statementContext) {}

// EnterRef_list is called when production ref_list is entered.
func (s *BaseDb2ParserListener) EnterRef_list(ctx *Ref_listContext) {}

// ExitRef_list is called when production ref_list is exited.
func (s *BaseDb2ParserListener) ExitRef_list(ctx *Ref_listContext) {}

// EnterRef_list_item is called when production ref_list_item is entered.
func (s *BaseDb2ParserListener) EnterRef_list_item(ctx *Ref_list_itemContext) {}

// ExitRef_list_item is called when production ref_list_item is exited.
func (s *BaseDb2ParserListener) ExitRef_list_item(ctx *Ref_list_itemContext) {}

// EnterOld_new is called when production old_new is entered.
func (s *BaseDb2ParserListener) EnterOld_new(ctx *Old_newContext) {}

// ExitOld_new is called when production old_new is exited.
func (s *BaseDb2ParserListener) ExitOld_new(ctx *Old_newContext) {}

// EnterCorrelation_name is called when production correlation_name is entered.
func (s *BaseDb2ParserListener) EnterCorrelation_name(ctx *Correlation_nameContext) {}

// ExitCorrelation_name is called when production correlation_name is exited.
func (s *BaseDb2ParserListener) ExitCorrelation_name(ctx *Correlation_nameContext) {}

// EnterIdentifier is called when production identifier is entered.
func (s *BaseDb2ParserListener) EnterIdentifier(ctx *IdentifierContext) {}

// ExitIdentifier is called when production identifier is exited.
func (s *BaseDb2ParserListener) ExitIdentifier(ctx *IdentifierContext) {}

// EnterTrigger_event is called when production trigger_event is entered.
func (s *BaseDb2ParserListener) EnterTrigger_event(ctx *Trigger_eventContext) {}

// ExitTrigger_event is called when production trigger_event is exited.
func (s *BaseDb2ParserListener) ExitTrigger_event(ctx *Trigger_eventContext) {}

// EnterTriggered_action is called when production triggered_action is entered.
func (s *BaseDb2ParserListener) EnterTriggered_action(ctx *Triggered_actionContext) {}

// ExitTriggered_action is called when production triggered_action is exited.
func (s *BaseDb2ParserListener) ExitTriggered_action(ctx *Triggered_actionContext) {}

// EnterSql_procedure_statement is called when production sql_procedure_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_procedure_statement(ctx *Sql_procedure_statementContext) {}

// ExitSql_procedure_statement is called when production sql_procedure_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_procedure_statement(ctx *Sql_procedure_statementContext) {}

// EnterSql_function_statement is called when production sql_function_statement is entered.
func (s *BaseDb2ParserListener) EnterSql_function_statement(ctx *Sql_function_statementContext) {}

// ExitSql_function_statement is called when production sql_function_statement is exited.
func (s *BaseDb2ParserListener) ExitSql_function_statement(ctx *Sql_function_statementContext) {}

// EnterCreate_trusted_context_statement is called when production create_trusted_context_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_trusted_context_statement(ctx *Create_trusted_context_statementContext) {
}

// ExitCreate_trusted_context_statement is called when production create_trusted_context_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_trusted_context_statement(ctx *Create_trusted_context_statementContext) {
}

// EnterAttr_list is called when production attr_list is entered.
func (s *BaseDb2ParserListener) EnterAttr_list(ctx *Attr_listContext) {}

// ExitAttr_list is called when production attr_list is exited.
func (s *BaseDb2ParserListener) ExitAttr_list(ctx *Attr_listContext) {}

// EnterAttr_list_item is called when production attr_list_item is entered.
func (s *BaseDb2ParserListener) EnterAttr_list_item(ctx *Attr_list_itemContext) {}

// ExitAttr_list_item is called when production attr_list_item is exited.
func (s *BaseDb2ParserListener) ExitAttr_list_item(ctx *Attr_list_itemContext) {}

// EnterAuth_list is called when production auth_list is entered.
func (s *BaseDb2ParserListener) EnterAuth_list(ctx *Auth_listContext) {}

// ExitAuth_list is called when production auth_list is exited.
func (s *BaseDb2ParserListener) ExitAuth_list(ctx *Auth_listContext) {}

// EnterAuth_list_item is called when production auth_list_item is entered.
func (s *BaseDb2ParserListener) EnterAuth_list_item(ctx *Auth_list_itemContext) {}

// ExitAuth_list_item is called when production auth_list_item is exited.
func (s *BaseDb2ParserListener) ExitAuth_list_item(ctx *Auth_list_itemContext) {}

// EnterAddress_value is called when production address_value is entered.
func (s *BaseDb2ParserListener) EnterAddress_value(ctx *Address_valueContext) {}

// ExitAddress_value is called when production address_value is exited.
func (s *BaseDb2ParserListener) ExitAddress_value(ctx *Address_valueContext) {}

// EnterEncryption_value is called when production encryption_value is entered.
func (s *BaseDb2ParserListener) EnterEncryption_value(ctx *Encryption_valueContext) {}

// ExitEncryption_value is called when production encryption_value is exited.
func (s *BaseDb2ParserListener) ExitEncryption_value(ctx *Encryption_valueContext) {}

// EnterCreate_type_statement is called when production create_type_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_type_statement(ctx *Create_type_statementContext) {}

// ExitCreate_type_statement is called when production create_type_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_type_statement(ctx *Create_type_statementContext) {}

// EnterCreate_type_array_statement is called when production create_type_array_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_type_array_statement(ctx *Create_type_array_statementContext) {
}

// ExitCreate_type_array_statement is called when production create_type_array_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_type_array_statement(ctx *Create_type_array_statementContext) {
}

// EnterCreate_type_cursor_statement is called when production create_type_cursor_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_type_cursor_statement(ctx *Create_type_cursor_statementContext) {
}

// ExitCreate_type_cursor_statement is called when production create_type_cursor_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_type_cursor_statement(ctx *Create_type_cursor_statementContext) {
}

// EnterCreate_type_distinct_statement is called when production create_type_distinct_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_type_distinct_statement(ctx *Create_type_distinct_statementContext) {
}

// ExitCreate_type_distinct_statement is called when production create_type_distinct_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_type_distinct_statement(ctx *Create_type_distinct_statementContext) {
}

// EnterCreate_type_row_statement is called when production create_type_row_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_type_row_statement(ctx *Create_type_row_statementContext) {
}

// ExitCreate_type_row_statement is called when production create_type_row_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_type_row_statement(ctx *Create_type_row_statementContext) {
}

// EnterField_definition_list_paren is called when production field_definition_list_paren is entered.
func (s *BaseDb2ParserListener) EnterField_definition_list_paren(ctx *Field_definition_list_parenContext) {
}

// ExitField_definition_list_paren is called when production field_definition_list_paren is exited.
func (s *BaseDb2ParserListener) ExitField_definition_list_paren(ctx *Field_definition_list_parenContext) {
}

// EnterField_definition_list is called when production field_definition_list is entered.
func (s *BaseDb2ParserListener) EnterField_definition_list(ctx *Field_definition_listContext) {}

// ExitField_definition_list is called when production field_definition_list is exited.
func (s *BaseDb2ParserListener) ExitField_definition_list(ctx *Field_definition_listContext) {}

// EnterField_definition is called when production field_definition is entered.
func (s *BaseDb2ParserListener) EnterField_definition(ctx *Field_definitionContext) {}

// ExitField_definition is called when production field_definition is exited.
func (s *BaseDb2ParserListener) ExitField_definition(ctx *Field_definitionContext) {}

// EnterCreate_type_structured_statement is called when production create_type_structured_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_type_structured_statement(ctx *Create_type_structured_statementContext) {
}

// ExitCreate_type_structured_statement is called when production create_type_structured_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_type_structured_statement(ctx *Create_type_structured_statementContext) {
}

// EnterStructured_type_seq is called when production structured_type_seq is entered.
func (s *BaseDb2ParserListener) EnterStructured_type_seq(ctx *Structured_type_seqContext) {}

// ExitStructured_type_seq is called when production structured_type_seq is exited.
func (s *BaseDb2ParserListener) ExitStructured_type_seq(ctx *Structured_type_seqContext) {}

// EnterAttribute_definition_list_paren is called when production attribute_definition_list_paren is entered.
func (s *BaseDb2ParserListener) EnterAttribute_definition_list_paren(ctx *Attribute_definition_list_parenContext) {
}

// ExitAttribute_definition_list_paren is called when production attribute_definition_list_paren is exited.
func (s *BaseDb2ParserListener) ExitAttribute_definition_list_paren(ctx *Attribute_definition_list_parenContext) {
}

// EnterAttribute_definition_list is called when production attribute_definition_list is entered.
func (s *BaseDb2ParserListener) EnterAttribute_definition_list(ctx *Attribute_definition_listContext) {
}

// ExitAttribute_definition_list is called when production attribute_definition_list is exited.
func (s *BaseDb2ParserListener) ExitAttribute_definition_list(ctx *Attribute_definition_listContext) {
}

// EnterAttribute_definition is called when production attribute_definition is entered.
func (s *BaseDb2ParserListener) EnterAttribute_definition(ctx *Attribute_definitionContext) {}

// ExitAttribute_definition is called when production attribute_definition is exited.
func (s *BaseDb2ParserListener) ExitAttribute_definition(ctx *Attribute_definitionContext) {}

// EnterMethod_specification_list is called when production method_specification_list is entered.
func (s *BaseDb2ParserListener) EnterMethod_specification_list(ctx *Method_specification_listContext) {
}

// ExitMethod_specification_list is called when production method_specification_list is exited.
func (s *BaseDb2ParserListener) ExitMethod_specification_list(ctx *Method_specification_listContext) {
}

// EnterMethod_specification is called when production method_specification is entered.
func (s *BaseDb2ParserListener) EnterMethod_specification(ctx *Method_specificationContext) {}

// ExitMethod_specification is called when production method_specification is exited.
func (s *BaseDb2ParserListener) ExitMethod_specification(ctx *Method_specificationContext) {}

// EnterMethod_specification_seq is called when production method_specification_seq is entered.
func (s *BaseDb2ParserListener) EnterMethod_specification_seq(ctx *Method_specification_seqContext) {}

// ExitMethod_specification_seq is called when production method_specification_seq is exited.
func (s *BaseDb2ParserListener) ExitMethod_specification_seq(ctx *Method_specification_seqContext) {}

// EnterAs_locator is called when production as_locator is entered.
func (s *BaseDb2ParserListener) EnterAs_locator(ctx *As_locatorContext) {}

// ExitAs_locator is called when production as_locator is exited.
func (s *BaseDb2ParserListener) ExitAs_locator(ctx *As_locatorContext) {}

// EnterParam_decl_list_paren is called when production param_decl_list_paren is entered.
func (s *BaseDb2ParserListener) EnterParam_decl_list_paren(ctx *Param_decl_list_parenContext) {}

// ExitParam_decl_list_paren is called when production param_decl_list_paren is exited.
func (s *BaseDb2ParserListener) ExitParam_decl_list_paren(ctx *Param_decl_list_parenContext) {}

// EnterParam_decl_list is called when production param_decl_list is entered.
func (s *BaseDb2ParserListener) EnterParam_decl_list(ctx *Param_decl_listContext) {}

// ExitParam_decl_list is called when production param_decl_list is exited.
func (s *BaseDb2ParserListener) ExitParam_decl_list(ctx *Param_decl_listContext) {}

// EnterParam_decl is called when production param_decl is entered.
func (s *BaseDb2ParserListener) EnterParam_decl(ctx *Param_declContext) {}

// ExitParam_decl is called when production param_decl is exited.
func (s *BaseDb2ParserListener) ExitParam_decl(ctx *Param_declContext) {}

// EnterSql_routine_characteristics is called when production sql_routine_characteristics is entered.
func (s *BaseDb2ParserListener) EnterSql_routine_characteristics(ctx *Sql_routine_characteristicsContext) {
}

// ExitSql_routine_characteristics is called when production sql_routine_characteristics is exited.
func (s *BaseDb2ParserListener) ExitSql_routine_characteristics(ctx *Sql_routine_characteristicsContext) {
}

// EnterExternal_routine_characteristics is called when production external_routine_characteristics is entered.
func (s *BaseDb2ParserListener) EnterExternal_routine_characteristics(ctx *External_routine_characteristicsContext) {
}

// ExitExternal_routine_characteristics is called when production external_routine_characteristics is exited.
func (s *BaseDb2ParserListener) ExitExternal_routine_characteristics(ctx *External_routine_characteristicsContext) {
}

// EnterLength is called when production length is entered.
func (s *BaseDb2ParserListener) EnterLength(ctx *LengthContext) {}

// ExitLength is called when production length is exited.
func (s *BaseDb2ParserListener) ExitLength(ctx *LengthContext) {}

// EnterRep_type is called when production rep_type is entered.
func (s *BaseDb2ParserListener) EnterRep_type(ctx *Rep_typeContext) {}

// ExitRep_type is called when production rep_type is exited.
func (s *BaseDb2ParserListener) ExitRep_type(ctx *Rep_typeContext) {}

// EnterVarchars is called when production varchars is entered.
func (s *BaseDb2ParserListener) EnterVarchars(ctx *VarcharsContext) {}

// ExitVarchars is called when production varchars is exited.
func (s *BaseDb2ParserListener) ExitVarchars(ctx *VarcharsContext) {}

// EnterVarbinaries is called when production varbinaries is entered.
func (s *BaseDb2ParserListener) EnterVarbinaries(ctx *VarbinariesContext) {}

// ExitVarbinaries is called when production varbinaries is exited.
func (s *BaseDb2ParserListener) ExitVarbinaries(ctx *VarbinariesContext) {}

// EnterFor_bit_data is called when production for_bit_data is entered.
func (s *BaseDb2ParserListener) EnterFor_bit_data(ctx *For_bit_dataContext) {}

// ExitFor_bit_data is called when production for_bit_data is exited.
func (s *BaseDb2ParserListener) ExitFor_bit_data(ctx *For_bit_dataContext) {}

// EnterLob_options is called when production lob_options is entered.
func (s *BaseDb2ParserListener) EnterLob_options(ctx *Lob_optionsContext) {}

// ExitLob_options is called when production lob_options is exited.
func (s *BaseDb2ParserListener) ExitLob_options(ctx *Lob_optionsContext) {}

// EnterCreate_type_mapping_statement is called when production create_type_mapping_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_type_mapping_statement(ctx *Create_type_mapping_statementContext) {
}

// ExitCreate_type_mapping_statement is called when production create_type_mapping_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_type_mapping_statement(ctx *Create_type_mapping_statementContext) {
}

// EnterFor_bit_data_precision is called when production for_bit_data_precision is entered.
func (s *BaseDb2ParserListener) EnterFor_bit_data_precision(ctx *For_bit_data_precisionContext) {}

// ExitFor_bit_data_precision is called when production for_bit_data_precision is exited.
func (s *BaseDb2ParserListener) ExitFor_bit_data_precision(ctx *For_bit_data_precisionContext) {}

// EnterPrecision is called when production precision is entered.
func (s *BaseDb2ParserListener) EnterPrecision(ctx *PrecisionContext) {}

// ExitPrecision is called when production precision is exited.
func (s *BaseDb2ParserListener) ExitPrecision(ctx *PrecisionContext) {}

// EnterScale is called when production scale is entered.
func (s *BaseDb2ParserListener) EnterScale(ctx *ScaleContext) {}

// ExitScale is called when production scale is exited.
func (s *BaseDb2ParserListener) ExitScale(ctx *ScaleContext) {}

// EnterPrecision_scale_comp is called when production precision_scale_comp is entered.
func (s *BaseDb2ParserListener) EnterPrecision_scale_comp(ctx *Precision_scale_compContext) {}

// ExitPrecision_scale_comp is called when production precision_scale_comp is exited.
func (s *BaseDb2ParserListener) ExitPrecision_scale_comp(ctx *Precision_scale_compContext) {}

// EnterFrom_to is called when production from_to is entered.
func (s *BaseDb2ParserListener) EnterFrom_to(ctx *From_toContext) {}

// ExitFrom_to is called when production from_to is exited.
func (s *BaseDb2ParserListener) ExitFrom_to(ctx *From_toContext) {}

// EnterData_source_data_type is called when production data_source_data_type is entered.
func (s *BaseDb2ParserListener) EnterData_source_data_type(ctx *Data_source_data_typeContext) {}

// ExitData_source_data_type is called when production data_source_data_type is exited.
func (s *BaseDb2ParserListener) ExitData_source_data_type(ctx *Data_source_data_typeContext) {}

// EnterLocal_data_type is called when production local_data_type is entered.
func (s *BaseDb2ParserListener) EnterLocal_data_type(ctx *Local_data_typeContext) {}

// ExitLocal_data_type is called when production local_data_type is exited.
func (s *BaseDb2ParserListener) ExitLocal_data_type(ctx *Local_data_typeContext) {}

// EnterRemote_server is called when production remote_server is entered.
func (s *BaseDb2ParserListener) EnterRemote_server(ctx *Remote_serverContext) {}

// ExitRemote_server is called when production remote_server is exited.
func (s *BaseDb2ParserListener) ExitRemote_server(ctx *Remote_serverContext) {}

// EnterServer_version is called when production server_version is entered.
func (s *BaseDb2ParserListener) EnterServer_version(ctx *Server_versionContext) {}

// ExitServer_version is called when production server_version is exited.
func (s *BaseDb2ParserListener) ExitServer_version(ctx *Server_versionContext) {}

// EnterServer_type is called when production server_type is entered.
func (s *BaseDb2ParserListener) EnterServer_type(ctx *Server_typeContext) {}

// ExitServer_type is called when production server_type is exited.
func (s *BaseDb2ParserListener) ExitServer_type(ctx *Server_typeContext) {}

// EnterVersion is called when production version is entered.
func (s *BaseDb2ParserListener) EnterVersion(ctx *VersionContext) {}

// ExitVersion is called when production version is exited.
func (s *BaseDb2ParserListener) ExitVersion(ctx *VersionContext) {}

// EnterRelease is called when production release is entered.
func (s *BaseDb2ParserListener) EnterRelease(ctx *ReleaseContext) {}

// ExitRelease is called when production release is exited.
func (s *BaseDb2ParserListener) ExitRelease(ctx *ReleaseContext) {}

// EnterMod is called when production mod is entered.
func (s *BaseDb2ParserListener) EnterMod(ctx *ModContext) {}

// ExitMod is called when production mod is exited.
func (s *BaseDb2ParserListener) ExitMod(ctx *ModContext) {}

// EnterCreate_usage_list_statement is called when production create_usage_list_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_usage_list_statement(ctx *Create_usage_list_statementContext) {
}

// ExitCreate_usage_list_statement is called when production create_usage_list_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_usage_list_statement(ctx *Create_usage_list_statementContext) {
}

// EnterCreate_user_mapping_statement is called when production create_user_mapping_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_user_mapping_statement(ctx *Create_user_mapping_statementContext) {
}

// ExitCreate_user_mapping_statement is called when production create_user_mapping_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_user_mapping_statement(ctx *Create_user_mapping_statementContext) {
}

// EnterUser_mapping_options_paren is called when production user_mapping_options_paren is entered.
func (s *BaseDb2ParserListener) EnterUser_mapping_options_paren(ctx *User_mapping_options_parenContext) {
}

// ExitUser_mapping_options_paren is called when production user_mapping_options_paren is exited.
func (s *BaseDb2ParserListener) ExitUser_mapping_options_paren(ctx *User_mapping_options_parenContext) {
}

// EnterUser_mapping_options is called when production user_mapping_options is entered.
func (s *BaseDb2ParserListener) EnterUser_mapping_options(ctx *User_mapping_optionsContext) {}

// ExitUser_mapping_options is called when production user_mapping_options is exited.
func (s *BaseDb2ParserListener) ExitUser_mapping_options(ctx *User_mapping_optionsContext) {}

// EnterCreate_variable_statement is called when production create_variable_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_variable_statement(ctx *Create_variable_statementContext) {
}

// ExitCreate_variable_statement is called when production create_variable_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_variable_statement(ctx *Create_variable_statementContext) {
}

// EnterConstant_ is called when production constant_ is entered.
func (s *BaseDb2ParserListener) EnterConstant_(ctx *Constant_Context) {}

// ExitConstant_ is called when production constant_ is exited.
func (s *BaseDb2ParserListener) ExitConstant_(ctx *Constant_Context) {}

// EnterSpecial_register is called when production special_register is entered.
func (s *BaseDb2ParserListener) EnterSpecial_register(ctx *Special_registerContext) {}

// ExitSpecial_register is called when production special_register is exited.
func (s *BaseDb2ParserListener) ExitSpecial_register(ctx *Special_registerContext) {}

// EnterGlobal_variable is called when production global_variable is entered.
func (s *BaseDb2ParserListener) EnterGlobal_variable(ctx *Global_variableContext) {}

// ExitGlobal_variable is called when production global_variable is exited.
func (s *BaseDb2ParserListener) ExitGlobal_variable(ctx *Global_variableContext) {}

// EnterData_type_1 is called when production data_type_1 is entered.
func (s *BaseDb2ParserListener) EnterData_type_1(ctx *Data_type_1Context) {}

// ExitData_type_1 is called when production data_type_1 is exited.
func (s *BaseDb2ParserListener) ExitData_type_1(ctx *Data_type_1Context) {}

// EnterCursor_value_constructor is called when production cursor_value_constructor is entered.
func (s *BaseDb2ParserListener) EnterCursor_value_constructor(ctx *Cursor_value_constructorContext) {}

// ExitCursor_value_constructor is called when production cursor_value_constructor is exited.
func (s *BaseDb2ParserListener) ExitCursor_value_constructor(ctx *Cursor_value_constructorContext) {}

// EnterAnchored_variable_data_type is called when production anchored_variable_data_type is entered.
func (s *BaseDb2ParserListener) EnterAnchored_variable_data_type(ctx *Anchored_variable_data_typeContext) {
}

// ExitAnchored_variable_data_type is called when production anchored_variable_data_type is exited.
func (s *BaseDb2ParserListener) ExitAnchored_variable_data_type(ctx *Anchored_variable_data_typeContext) {
}

// EnterHoldability is called when production holdability is entered.
func (s *BaseDb2ParserListener) EnterHoldability(ctx *HoldabilityContext) {}

// ExitHoldability is called when production holdability is exited.
func (s *BaseDb2ParserListener) ExitHoldability(ctx *HoldabilityContext) {}

// EnterReturnability is called when production returnability is entered.
func (s *BaseDb2ParserListener) EnterReturnability(ctx *ReturnabilityContext) {}

// ExitReturnability is called when production returnability is exited.
func (s *BaseDb2ParserListener) ExitReturnability(ctx *ReturnabilityContext) {}

// EnterCreate_view_statement is called when production create_view_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_view_statement(ctx *Create_view_statementContext) {}

// ExitCreate_view_statement is called when production create_view_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_view_statement(ctx *Create_view_statementContext) {}

// EnterCreate_view_seq is called when production create_view_seq is entered.
func (s *BaseDb2ParserListener) EnterCreate_view_seq(ctx *Create_view_seqContext) {}

// ExitCreate_view_seq is called when production create_view_seq is exited.
func (s *BaseDb2ParserListener) ExitCreate_view_seq(ctx *Create_view_seqContext) {}

// EnterFullselect is called when production fullselect is entered.
func (s *BaseDb2ParserListener) EnterFullselect(ctx *FullselectContext) {}

// ExitFullselect is called when production fullselect is exited.
func (s *BaseDb2ParserListener) ExitFullselect(ctx *FullselectContext) {}

// EnterSubselect is called when production subselect is entered.
func (s *BaseDb2ParserListener) EnterSubselect(ctx *SubselectContext) {}

// ExitSubselect is called when production subselect is exited.
func (s *BaseDb2ParserListener) ExitSubselect(ctx *SubselectContext) {}

// EnterSelect_clause is called when production select_clause is entered.
func (s *BaseDb2ParserListener) EnterSelect_clause(ctx *Select_clauseContext) {}

// ExitSelect_clause is called when production select_clause is exited.
func (s *BaseDb2ParserListener) ExitSelect_clause(ctx *Select_clauseContext) {}

// EnterSelect_clause_item is called when production select_clause_item is entered.
func (s *BaseDb2ParserListener) EnterSelect_clause_item(ctx *Select_clause_itemContext) {}

// ExitSelect_clause_item is called when production select_clause_item is exited.
func (s *BaseDb2ParserListener) ExitSelect_clause_item(ctx *Select_clause_itemContext) {}

// EnterFrom_clause is called when production from_clause is entered.
func (s *BaseDb2ParserListener) EnterFrom_clause(ctx *From_clauseContext) {}

// ExitFrom_clause is called when production from_clause is exited.
func (s *BaseDb2ParserListener) ExitFrom_clause(ctx *From_clauseContext) {}

// EnterTable_reference is called when production table_reference is entered.
func (s *BaseDb2ParserListener) EnterTable_reference(ctx *Table_referenceContext) {}

// ExitTable_reference is called when production table_reference is exited.
func (s *BaseDb2ParserListener) ExitTable_reference(ctx *Table_referenceContext) {}

// EnterTable_reference_list is called when production table_reference_list is entered.
func (s *BaseDb2ParserListener) EnterTable_reference_list(ctx *Table_reference_listContext) {}

// ExitTable_reference_list is called when production table_reference_list is exited.
func (s *BaseDb2ParserListener) ExitTable_reference_list(ctx *Table_reference_listContext) {}

// EnterSingles_table_reference is called when production singles_table_reference is entered.
func (s *BaseDb2ParserListener) EnterSingles_table_reference(ctx *Singles_table_referenceContext) {}

// ExitSingles_table_reference is called when production singles_table_reference is exited.
func (s *BaseDb2ParserListener) ExitSingles_table_reference(ctx *Singles_table_referenceContext) {}

// EnterPeriod_specification is called when production period_specification is entered.
func (s *BaseDb2ParserListener) EnterPeriod_specification(ctx *Period_specificationContext) {}

// ExitPeriod_specification is called when production period_specification is exited.
func (s *BaseDb2ParserListener) ExitPeriod_specification(ctx *Period_specificationContext) {}

// EnterValue is called when production value is entered.
func (s *BaseDb2ParserListener) EnterValue(ctx *ValueContext) {}

// ExitValue is called when production value is exited.
func (s *BaseDb2ParserListener) ExitValue(ctx *ValueContext) {}

// EnterCorrelation_clause is called when production correlation_clause is entered.
func (s *BaseDb2ParserListener) EnterCorrelation_clause(ctx *Correlation_clauseContext) {}

// ExitCorrelation_clause is called when production correlation_clause is exited.
func (s *BaseDb2ParserListener) ExitCorrelation_clause(ctx *Correlation_clauseContext) {}

// EnterTablesample_clause is called when production tablesample_clause is entered.
func (s *BaseDb2ParserListener) EnterTablesample_clause(ctx *Tablesample_clauseContext) {}

// ExitTablesample_clause is called when production tablesample_clause is exited.
func (s *BaseDb2ParserListener) ExitTablesample_clause(ctx *Tablesample_clauseContext) {}

// EnterNumeric_expression is called when production numeric_expression is entered.
func (s *BaseDb2ParserListener) EnterNumeric_expression(ctx *Numeric_expressionContext) {}

// ExitNumeric_expression is called when production numeric_expression is exited.
func (s *BaseDb2ParserListener) ExitNumeric_expression(ctx *Numeric_expressionContext) {}

// EnterSingle_view_reference is called when production single_view_reference is entered.
func (s *BaseDb2ParserListener) EnterSingle_view_reference(ctx *Single_view_referenceContext) {}

// ExitSingle_view_reference is called when production single_view_reference is exited.
func (s *BaseDb2ParserListener) ExitSingle_view_reference(ctx *Single_view_referenceContext) {}

// EnterSingle_nickname_reference is called when production single_nickname_reference is entered.
func (s *BaseDb2ParserListener) EnterSingle_nickname_reference(ctx *Single_nickname_referenceContext) {
}

// ExitSingle_nickname_reference is called when production single_nickname_reference is exited.
func (s *BaseDb2ParserListener) ExitSingle_nickname_reference(ctx *Single_nickname_referenceContext) {
}

// EnterOnly_table_reference is called when production only_table_reference is entered.
func (s *BaseDb2ParserListener) EnterOnly_table_reference(ctx *Only_table_referenceContext) {}

// ExitOnly_table_reference is called when production only_table_reference is exited.
func (s *BaseDb2ParserListener) ExitOnly_table_reference(ctx *Only_table_referenceContext) {}

// EnterOuter_table_reference is called when production outer_table_reference is entered.
func (s *BaseDb2ParserListener) EnterOuter_table_reference(ctx *Outer_table_referenceContext) {}

// ExitOuter_table_reference is called when production outer_table_reference is exited.
func (s *BaseDb2ParserListener) ExitOuter_table_reference(ctx *Outer_table_referenceContext) {}

// EnterAnalyze_table_reference is called when production analyze_table_reference is entered.
func (s *BaseDb2ParserListener) EnterAnalyze_table_reference(ctx *Analyze_table_referenceContext) {}

// ExitAnalyze_table_reference is called when production analyze_table_reference is exited.
func (s *BaseDb2ParserListener) ExitAnalyze_table_reference(ctx *Analyze_table_referenceContext) {}

// EnterImplementation_clause is called when production implementation_clause is entered.
func (s *BaseDb2ParserListener) EnterImplementation_clause(ctx *Implementation_clauseContext) {}

// ExitImplementation_clause is called when production implementation_clause is exited.
func (s *BaseDb2ParserListener) ExitImplementation_clause(ctx *Implementation_clauseContext) {}

// EnterNested_table_reference is called when production nested_table_reference is entered.
func (s *BaseDb2ParserListener) EnterNested_table_reference(ctx *Nested_table_referenceContext) {}

// ExitNested_table_reference is called when production nested_table_reference is exited.
func (s *BaseDb2ParserListener) ExitNested_table_reference(ctx *Nested_table_referenceContext) {}

// EnterContinue_handler is called when production continue_handler is entered.
func (s *BaseDb2ParserListener) EnterContinue_handler(ctx *Continue_handlerContext) {}

// ExitContinue_handler is called when production continue_handler is exited.
func (s *BaseDb2ParserListener) ExitContinue_handler(ctx *Continue_handlerContext) {}

// EnterSpecific_condition_value is called when production specific_condition_value is entered.
func (s *BaseDb2ParserListener) EnterSpecific_condition_value(ctx *Specific_condition_valueContext) {}

// ExitSpecific_condition_value is called when production specific_condition_value is exited.
func (s *BaseDb2ParserListener) ExitSpecific_condition_value(ctx *Specific_condition_valueContext) {}

// EnterData_change_table_reference is called when production data_change_table_reference is entered.
func (s *BaseDb2ParserListener) EnterData_change_table_reference(ctx *Data_change_table_referenceContext) {
}

// ExitData_change_table_reference is called when production data_change_table_reference is exited.
func (s *BaseDb2ParserListener) ExitData_change_table_reference(ctx *Data_change_table_referenceContext) {
}

// EnterSearched_update_statement is called when production searched_update_statement is entered.
func (s *BaseDb2ParserListener) EnterSearched_update_statement(ctx *Searched_update_statementContext) {
}

// ExitSearched_update_statement is called when production searched_update_statement is exited.
func (s *BaseDb2ParserListener) ExitSearched_update_statement(ctx *Searched_update_statementContext) {
}

// EnterSearched_delete_statement is called when production searched_delete_statement is entered.
func (s *BaseDb2ParserListener) EnterSearched_delete_statement(ctx *Searched_delete_statementContext) {
}

// ExitSearched_delete_statement is called when production searched_delete_statement is exited.
func (s *BaseDb2ParserListener) ExitSearched_delete_statement(ctx *Searched_delete_statementContext) {
}

// EnterFinal_new is called when production final_new is entered.
func (s *BaseDb2ParserListener) EnterFinal_new(ctx *Final_newContext) {}

// ExitFinal_new is called when production final_new is exited.
func (s *BaseDb2ParserListener) ExitFinal_new(ctx *Final_newContext) {}

// EnterFinal_new_old is called when production final_new_old is entered.
func (s *BaseDb2ParserListener) EnterFinal_new_old(ctx *Final_new_oldContext) {}

// ExitFinal_new_old is called when production final_new_old is exited.
func (s *BaseDb2ParserListener) ExitFinal_new_old(ctx *Final_new_oldContext) {}

// EnterTable_function_reference is called when production table_function_reference is entered.
func (s *BaseDb2ParserListener) EnterTable_function_reference(ctx *Table_function_referenceContext) {}

// ExitTable_function_reference is called when production table_function_reference is exited.
func (s *BaseDb2ParserListener) ExitTable_function_reference(ctx *Table_function_referenceContext) {}

// EnterTable_udf_cardinality_clause is called when production table_udf_cardinality_clause is entered.
func (s *BaseDb2ParserListener) EnterTable_udf_cardinality_clause(ctx *Table_udf_cardinality_clauseContext) {
}

// ExitTable_udf_cardinality_clause is called when production table_udf_cardinality_clause is exited.
func (s *BaseDb2ParserListener) ExitTable_udf_cardinality_clause(ctx *Table_udf_cardinality_clauseContext) {
}

// EnterTyped_correlation_clause is called when production typed_correlation_clause is entered.
func (s *BaseDb2ParserListener) EnterTyped_correlation_clause(ctx *Typed_correlation_clauseContext) {}

// ExitTyped_correlation_clause is called when production typed_correlation_clause is exited.
func (s *BaseDb2ParserListener) ExitTyped_correlation_clause(ctx *Typed_correlation_clauseContext) {}

// EnterColumn_name_data_type is called when production column_name_data_type is entered.
func (s *BaseDb2ParserListener) EnterColumn_name_data_type(ctx *Column_name_data_typeContext) {}

// ExitColumn_name_data_type is called when production column_name_data_type is exited.
func (s *BaseDb2ParserListener) ExitColumn_name_data_type(ctx *Column_name_data_typeContext) {}

// EnterCollection_derived_table is called when production collection_derived_table is entered.
func (s *BaseDb2ParserListener) EnterCollection_derived_table(ctx *Collection_derived_tableContext) {}

// ExitCollection_derived_table is called when production collection_derived_table is exited.
func (s *BaseDb2ParserListener) ExitCollection_derived_table(ctx *Collection_derived_tableContext) {}

// EnterTable_function is called when production table_function is entered.
func (s *BaseDb2ParserListener) EnterTable_function(ctx *Table_functionContext) {}

// ExitTable_function is called when production table_function is exited.
func (s *BaseDb2ParserListener) ExitTable_function(ctx *Table_functionContext) {}

// EnterXmltable_expression is called when production xmltable_expression is entered.
func (s *BaseDb2ParserListener) EnterXmltable_expression(ctx *Xmltable_expressionContext) {}

// ExitXmltable_expression is called when production xmltable_expression is exited.
func (s *BaseDb2ParserListener) ExitXmltable_expression(ctx *Xmltable_expressionContext) {}

// EnterXmltable_function is called when production xmltable_function is entered.
func (s *BaseDb2ParserListener) EnterXmltable_function(ctx *Xmltable_functionContext) {}

// ExitXmltable_function is called when production xmltable_function is exited.
func (s *BaseDb2ParserListener) ExitXmltable_function(ctx *Xmltable_functionContext) {}

// EnterJoined_table is called when production joined_table is entered.
func (s *BaseDb2ParserListener) EnterJoined_table(ctx *Joined_tableContext) {}

// ExitJoined_table is called when production joined_table is exited.
func (s *BaseDb2ParserListener) ExitJoined_table(ctx *Joined_tableContext) {}

// EnterJoin_condition is called when production join_condition is entered.
func (s *BaseDb2ParserListener) EnterJoin_condition(ctx *Join_conditionContext) {}

// ExitJoin_condition is called when production join_condition is exited.
func (s *BaseDb2ParserListener) ExitJoin_condition(ctx *Join_conditionContext) {}

// EnterOuter is called when production outer is entered.
func (s *BaseDb2ParserListener) EnterOuter(ctx *OuterContext) {}

// ExitOuter is called when production outer is exited.
func (s *BaseDb2ParserListener) ExitOuter(ctx *OuterContext) {}

// EnterExternal_table_reference is called when production external_table_reference is entered.
func (s *BaseDb2ParserListener) EnterExternal_table_reference(ctx *External_table_referenceContext) {}

// ExitExternal_table_reference is called when production external_table_reference is exited.
func (s *BaseDb2ParserListener) ExitExternal_table_reference(ctx *External_table_referenceContext) {}

// EnterColumn_definition_2 is called when production column_definition_2 is entered.
func (s *BaseDb2ParserListener) EnterColumn_definition_2(ctx *Column_definition_2Context) {}

// ExitColumn_definition_2 is called when production column_definition_2 is exited.
func (s *BaseDb2ParserListener) ExitColumn_definition_2(ctx *Column_definition_2Context) {}

// EnterFile_name is called when production file_name is entered.
func (s *BaseDb2ParserListener) EnterFile_name(ctx *File_nameContext) {}

// ExitFile_name is called when production file_name is exited.
func (s *BaseDb2ParserListener) ExitFile_name(ctx *File_nameContext) {}

// EnterWhere_clause is called when production where_clause is entered.
func (s *BaseDb2ParserListener) EnterWhere_clause(ctx *Where_clauseContext) {}

// ExitWhere_clause is called when production where_clause is exited.
func (s *BaseDb2ParserListener) ExitWhere_clause(ctx *Where_clauseContext) {}

// EnterGroup_by_clause is called when production group_by_clause is entered.
func (s *BaseDb2ParserListener) EnterGroup_by_clause(ctx *Group_by_clauseContext) {}

// ExitGroup_by_clause is called when production group_by_clause is exited.
func (s *BaseDb2ParserListener) ExitGroup_by_clause(ctx *Group_by_clauseContext) {}

// EnterGroup_by_clause_opts is called when production group_by_clause_opts is entered.
func (s *BaseDb2ParserListener) EnterGroup_by_clause_opts(ctx *Group_by_clause_optsContext) {}

// ExitGroup_by_clause_opts is called when production group_by_clause_opts is exited.
func (s *BaseDb2ParserListener) ExitGroup_by_clause_opts(ctx *Group_by_clause_optsContext) {}

// EnterGrouping_expression is called when production grouping_expression is entered.
func (s *BaseDb2ParserListener) EnterGrouping_expression(ctx *Grouping_expressionContext) {}

// ExitGrouping_expression is called when production grouping_expression is exited.
func (s *BaseDb2ParserListener) ExitGrouping_expression(ctx *Grouping_expressionContext) {}

// EnterGrouping_sets is called when production grouping_sets is entered.
func (s *BaseDb2ParserListener) EnterGrouping_sets(ctx *Grouping_setsContext) {}

// ExitGrouping_sets is called when production grouping_sets is exited.
func (s *BaseDb2ParserListener) ExitGrouping_sets(ctx *Grouping_setsContext) {}

// EnterSuper_groups is called when production super_groups is entered.
func (s *BaseDb2ParserListener) EnterSuper_groups(ctx *Super_groupsContext) {}

// ExitSuper_groups is called when production super_groups is exited.
func (s *BaseDb2ParserListener) ExitSuper_groups(ctx *Super_groupsContext) {}

// EnterGrant_total is called when production grant_total is entered.
func (s *BaseDb2ParserListener) EnterGrant_total(ctx *Grant_totalContext) {}

// ExitGrant_total is called when production grant_total is exited.
func (s *BaseDb2ParserListener) ExitGrant_total(ctx *Grant_totalContext) {}

// EnterHaving_clause is called when production having_clause is entered.
func (s *BaseDb2ParserListener) EnterHaving_clause(ctx *Having_clauseContext) {}

// ExitHaving_clause is called when production having_clause is exited.
func (s *BaseDb2ParserListener) ExitHaving_clause(ctx *Having_clauseContext) {}

// EnterOrder_by_clause is called when production order_by_clause is entered.
func (s *BaseDb2ParserListener) EnterOrder_by_clause(ctx *Order_by_clauseContext) {}

// ExitOrder_by_clause is called when production order_by_clause is exited.
func (s *BaseDb2ParserListener) ExitOrder_by_clause(ctx *Order_by_clauseContext) {}

// EnterOrder_by_clause_opts is called when production order_by_clause_opts is entered.
func (s *BaseDb2ParserListener) EnterOrder_by_clause_opts(ctx *Order_by_clause_optsContext) {}

// ExitOrder_by_clause_opts is called when production order_by_clause_opts is exited.
func (s *BaseDb2ParserListener) ExitOrder_by_clause_opts(ctx *Order_by_clause_optsContext) {}

// EnterTable_designator is called when production table_designator is entered.
func (s *BaseDb2ParserListener) EnterTable_designator(ctx *Table_designatorContext) {}

// ExitTable_designator is called when production table_designator is exited.
func (s *BaseDb2ParserListener) ExitTable_designator(ctx *Table_designatorContext) {}

// EnterAsc_desc is called when production asc_desc is entered.
func (s *BaseDb2ParserListener) EnterAsc_desc(ctx *Asc_descContext) {}

// ExitAsc_desc is called when production asc_desc is exited.
func (s *BaseDb2ParserListener) ExitAsc_desc(ctx *Asc_descContext) {}

// EnterFirst_last is called when production first_last is entered.
func (s *BaseDb2ParserListener) EnterFirst_last(ctx *First_lastContext) {}

// ExitFirst_last is called when production first_last is exited.
func (s *BaseDb2ParserListener) ExitFirst_last(ctx *First_lastContext) {}

// EnterSort_key is called when production sort_key is entered.
func (s *BaseDb2ParserListener) EnterSort_key(ctx *Sort_keyContext) {}

// ExitSort_key is called when production sort_key is exited.
func (s *BaseDb2ParserListener) ExitSort_key(ctx *Sort_keyContext) {}

// EnterSimple_column_name is called when production simple_column_name is entered.
func (s *BaseDb2ParserListener) EnterSimple_column_name(ctx *Simple_column_nameContext) {}

// ExitSimple_column_name is called when production simple_column_name is exited.
func (s *BaseDb2ParserListener) ExitSimple_column_name(ctx *Simple_column_nameContext) {}

// EnterSimple_integer is called when production simple_integer is entered.
func (s *BaseDb2ParserListener) EnterSimple_integer(ctx *Simple_integerContext) {}

// ExitSimple_integer is called when production simple_integer is exited.
func (s *BaseDb2ParserListener) ExitSimple_integer(ctx *Simple_integerContext) {}

// EnterSork_key_expression is called when production sork_key_expression is entered.
func (s *BaseDb2ParserListener) EnterSork_key_expression(ctx *Sork_key_expressionContext) {}

// ExitSork_key_expression is called when production sork_key_expression is exited.
func (s *BaseDb2ParserListener) ExitSork_key_expression(ctx *Sork_key_expressionContext) {}

// EnterOffset_clause is called when production offset_clause is entered.
func (s *BaseDb2ParserListener) EnterOffset_clause(ctx *Offset_clauseContext) {}

// ExitOffset_clause is called when production offset_clause is exited.
func (s *BaseDb2ParserListener) ExitOffset_clause(ctx *Offset_clauseContext) {}

// EnterOffset_row_count is called when production offset_row_count is entered.
func (s *BaseDb2ParserListener) EnterOffset_row_count(ctx *Offset_row_countContext) {}

// ExitOffset_row_count is called when production offset_row_count is exited.
func (s *BaseDb2ParserListener) ExitOffset_row_count(ctx *Offset_row_countContext) {}

// EnterFetch_clause is called when production fetch_clause is entered.
func (s *BaseDb2ParserListener) EnterFetch_clause(ctx *Fetch_clauseContext) {}

// ExitFetch_clause is called when production fetch_clause is exited.
func (s *BaseDb2ParserListener) ExitFetch_clause(ctx *Fetch_clauseContext) {}

// EnterFetch_row_count is called when production fetch_row_count is entered.
func (s *BaseDb2ParserListener) EnterFetch_row_count(ctx *Fetch_row_countContext) {}

// ExitFetch_row_count is called when production fetch_row_count is exited.
func (s *BaseDb2ParserListener) ExitFetch_row_count(ctx *Fetch_row_countContext) {}

// EnterRow_rows is called when production row_rows is entered.
func (s *BaseDb2ParserListener) EnterRow_rows(ctx *Row_rowsContext) {}

// ExitRow_rows is called when production row_rows is exited.
func (s *BaseDb2ParserListener) ExitRow_rows(ctx *Row_rowsContext) {}

// EnterIsolation_clause is called when production isolation_clause is entered.
func (s *BaseDb2ParserListener) EnterIsolation_clause(ctx *Isolation_clauseContext) {}

// ExitIsolation_clause is called when production isolation_clause is exited.
func (s *BaseDb2ParserListener) ExitIsolation_clause(ctx *Isolation_clauseContext) {}

// EnterLock_request_clause is called when production lock_request_clause is entered.
func (s *BaseDb2ParserListener) EnterLock_request_clause(ctx *Lock_request_clauseContext) {}

// ExitLock_request_clause is called when production lock_request_clause is exited.
func (s *BaseDb2ParserListener) ExitLock_request_clause(ctx *Lock_request_clauseContext) {}

// EnterValues_clause is called when production values_clause is entered.
func (s *BaseDb2ParserListener) EnterValues_clause(ctx *Values_clauseContext) {}

// ExitValues_clause is called when production values_clause is exited.
func (s *BaseDb2ParserListener) ExitValues_clause(ctx *Values_clauseContext) {}

// EnterValues_row is called when production values_row is entered.
func (s *BaseDb2ParserListener) EnterValues_row(ctx *Values_rowContext) {}

// ExitValues_row is called when production values_row is exited.
func (s *BaseDb2ParserListener) ExitValues_row(ctx *Values_rowContext) {}

// EnterRoot_view_definition is called when production root_view_definition is entered.
func (s *BaseDb2ParserListener) EnterRoot_view_definition(ctx *Root_view_definitionContext) {}

// ExitRoot_view_definition is called when production root_view_definition is exited.
func (s *BaseDb2ParserListener) ExitRoot_view_definition(ctx *Root_view_definitionContext) {}

// EnterSubview_definition is called when production subview_definition is entered.
func (s *BaseDb2ParserListener) EnterSubview_definition(ctx *Subview_definitionContext) {}

// ExitSubview_definition is called when production subview_definition is exited.
func (s *BaseDb2ParserListener) ExitSubview_definition(ctx *Subview_definitionContext) {}

// EnterOid_column is called when production oid_column is entered.
func (s *BaseDb2ParserListener) EnterOid_column(ctx *Oid_columnContext) {}

// ExitOid_column is called when production oid_column is exited.
func (s *BaseDb2ParserListener) ExitOid_column(ctx *Oid_columnContext) {}

// EnterWith_options is called when production with_options is entered.
func (s *BaseDb2ParserListener) EnterWith_options(ctx *With_optionsContext) {}

// ExitWith_options is called when production with_options is exited.
func (s *BaseDb2ParserListener) ExitWith_options(ctx *With_optionsContext) {}

// EnterWith_option_def is called when production with_option_def is entered.
func (s *BaseDb2ParserListener) EnterWith_option_def(ctx *With_option_defContext) {}

// ExitWith_option_def is called when production with_option_def is exited.
func (s *BaseDb2ParserListener) ExitWith_option_def(ctx *With_option_defContext) {}

// EnterWith_option_scope_def is called when production with_option_scope_def is entered.
func (s *BaseDb2ParserListener) EnterWith_option_scope_def(ctx *With_option_scope_defContext) {}

// ExitWith_option_scope_def is called when production with_option_scope_def is exited.
func (s *BaseDb2ParserListener) ExitWith_option_scope_def(ctx *With_option_scope_defContext) {}

// EnterUnder_clause is called when production under_clause is entered.
func (s *BaseDb2ParserListener) EnterUnder_clause(ctx *Under_clauseContext) {}

// ExitUnder_clause is called when production under_clause is exited.
func (s *BaseDb2ParserListener) ExitUnder_clause(ctx *Under_clauseContext) {}

// EnterCreate_work_action_set_statement is called when production create_work_action_set_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_work_action_set_statement(ctx *Create_work_action_set_statementContext) {
}

// ExitCreate_work_action_set_statement is called when production create_work_action_set_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_work_action_set_statement(ctx *Create_work_action_set_statementContext) {
}

// EnterWork_action_definition_list_paren is called when production work_action_definition_list_paren is entered.
func (s *BaseDb2ParserListener) EnterWork_action_definition_list_paren(ctx *Work_action_definition_list_parenContext) {
}

// ExitWork_action_definition_list_paren is called when production work_action_definition_list_paren is exited.
func (s *BaseDb2ParserListener) ExitWork_action_definition_list_paren(ctx *Work_action_definition_list_parenContext) {
}

// EnterWork_action_definition_list is called when production work_action_definition_list is entered.
func (s *BaseDb2ParserListener) EnterWork_action_definition_list(ctx *Work_action_definition_listContext) {
}

// ExitWork_action_definition_list is called when production work_action_definition_list is exited.
func (s *BaseDb2ParserListener) ExitWork_action_definition_list(ctx *Work_action_definition_listContext) {
}

// EnterWork_action_definition is called when production work_action_definition is entered.
func (s *BaseDb2ParserListener) EnterWork_action_definition(ctx *Work_action_definitionContext) {}

// ExitWork_action_definition is called when production work_action_definition is exited.
func (s *BaseDb2ParserListener) ExitWork_action_definition(ctx *Work_action_definitionContext) {}

// EnterAction_types_clause is called when production action_types_clause is entered.
func (s *BaseDb2ParserListener) EnterAction_types_clause(ctx *Action_types_clauseContext) {}

// ExitAction_types_clause is called when production action_types_clause is exited.
func (s *BaseDb2ParserListener) ExitAction_types_clause(ctx *Action_types_clauseContext) {}

// EnterThreshold_types_clause is called when production threshold_types_clause is entered.
func (s *BaseDb2ParserListener) EnterThreshold_types_clause(ctx *Threshold_types_clauseContext) {}

// ExitThreshold_types_clause is called when production threshold_types_clause is exited.
func (s *BaseDb2ParserListener) ExitThreshold_types_clause(ctx *Threshold_types_clauseContext) {}

// EnterSecond_seconds is called when production second_seconds is entered.
func (s *BaseDb2ParserListener) EnterSecond_seconds(ctx *Second_secondsContext) {}

// ExitSecond_seconds is called when production second_seconds is exited.
func (s *BaseDb2ParserListener) ExitSecond_seconds(ctx *Second_secondsContext) {}

// EnterHours_minutes is called when production hours_minutes is entered.
func (s *BaseDb2ParserListener) EnterHours_minutes(ctx *Hours_minutesContext) {}

// ExitHours_minutes is called when production hours_minutes is exited.
func (s *BaseDb2ParserListener) ExitHours_minutes(ctx *Hours_minutesContext) {}

// EnterThreshold_exceeded_actions is called when production threshold_exceeded_actions is entered.
func (s *BaseDb2ParserListener) EnterThreshold_exceeded_actions(ctx *Threshold_exceeded_actionsContext) {
}

// ExitThreshold_exceeded_actions is called when production threshold_exceeded_actions is exited.
func (s *BaseDb2ParserListener) ExitThreshold_exceeded_actions(ctx *Threshold_exceeded_actionsContext) {
}

// EnterCollect_activity_data_clause is called when production collect_activity_data_clause is entered.
func (s *BaseDb2ParserListener) EnterCollect_activity_data_clause(ctx *Collect_activity_data_clauseContext) {
}

// ExitCollect_activity_data_clause is called when production collect_activity_data_clause is exited.
func (s *BaseDb2ParserListener) ExitCollect_activity_data_clause(ctx *Collect_activity_data_clauseContext) {
}

// EnterWith_without is called when production with_without is entered.
func (s *BaseDb2ParserListener) EnterWith_without(ctx *With_withoutContext) {}

// ExitWith_without is called when production with_without is exited.
func (s *BaseDb2ParserListener) ExitWith_without(ctx *With_withoutContext) {}

// EnterHistogram_templace_clause is called when production histogram_templace_clause is entered.
func (s *BaseDb2ParserListener) EnterHistogram_templace_clause(ctx *Histogram_templace_clauseContext) {
}

// ExitHistogram_templace_clause is called when production histogram_templace_clause is exited.
func (s *BaseDb2ParserListener) ExitHistogram_templace_clause(ctx *Histogram_templace_clauseContext) {
}

// EnterCreate_work_class_set_statement is called when production create_work_class_set_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_work_class_set_statement(ctx *Create_work_class_set_statementContext) {
}

// ExitCreate_work_class_set_statement is called when production create_work_class_set_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_work_class_set_statement(ctx *Create_work_class_set_statementContext) {
}

// EnterWork_class_definition_list_paren is called when production work_class_definition_list_paren is entered.
func (s *BaseDb2ParserListener) EnterWork_class_definition_list_paren(ctx *Work_class_definition_list_parenContext) {
}

// ExitWork_class_definition_list_paren is called when production work_class_definition_list_paren is exited.
func (s *BaseDb2ParserListener) ExitWork_class_definition_list_paren(ctx *Work_class_definition_list_parenContext) {
}

// EnterWork_class_definition_list is called when production work_class_definition_list is entered.
func (s *BaseDb2ParserListener) EnterWork_class_definition_list(ctx *Work_class_definition_listContext) {
}

// ExitWork_class_definition_list is called when production work_class_definition_list is exited.
func (s *BaseDb2ParserListener) ExitWork_class_definition_list(ctx *Work_class_definition_listContext) {
}

// EnterWork_class_definition is called when production work_class_definition is entered.
func (s *BaseDb2ParserListener) EnterWork_class_definition(ctx *Work_class_definitionContext) {}

// ExitWork_class_definition is called when production work_class_definition is exited.
func (s *BaseDb2ParserListener) ExitWork_class_definition(ctx *Work_class_definitionContext) {}

// EnterWork_attributes is called when production work_attributes is entered.
func (s *BaseDb2ParserListener) EnterWork_attributes(ctx *Work_attributesContext) {}

// ExitWork_attributes is called when production work_attributes is exited.
func (s *BaseDb2ParserListener) ExitWork_attributes(ctx *Work_attributesContext) {}

// EnterPosition_clause is called when production position_clause is entered.
func (s *BaseDb2ParserListener) EnterPosition_clause(ctx *Position_clauseContext) {}

// ExitPosition_clause is called when production position_clause is exited.
func (s *BaseDb2ParserListener) ExitPosition_clause(ctx *Position_clauseContext) {}

// EnterPosition_ is called when production position_ is entered.
func (s *BaseDb2ParserListener) EnterPosition_(ctx *Position_Context) {}

// ExitPosition_ is called when production position_ is exited.
func (s *BaseDb2ParserListener) ExitPosition_(ctx *Position_Context) {}

// EnterFor_from_to_clause is called when production for_from_to_clause is entered.
func (s *BaseDb2ParserListener) EnterFor_from_to_clause(ctx *For_from_to_clauseContext) {}

// ExitFor_from_to_clause is called when production for_from_to_clause is exited.
func (s *BaseDb2ParserListener) ExitFor_from_to_clause(ctx *For_from_to_clauseContext) {}

// EnterFrom_value is called when production from_value is entered.
func (s *BaseDb2ParserListener) EnterFrom_value(ctx *From_valueContext) {}

// ExitFrom_value is called when production from_value is exited.
func (s *BaseDb2ParserListener) ExitFrom_value(ctx *From_valueContext) {}

// EnterTo_value is called when production to_value is entered.
func (s *BaseDb2ParserListener) EnterTo_value(ctx *To_valueContext) {}

// ExitTo_value is called when production to_value is exited.
func (s *BaseDb2ParserListener) ExitTo_value(ctx *To_valueContext) {}

// EnterData_tag_clause is called when production data_tag_clause is entered.
func (s *BaseDb2ParserListener) EnterData_tag_clause(ctx *Data_tag_clauseContext) {}

// ExitData_tag_clause is called when production data_tag_clause is exited.
func (s *BaseDb2ParserListener) ExitData_tag_clause(ctx *Data_tag_clauseContext) {}

// EnterSchema_clause is called when production schema_clause is entered.
func (s *BaseDb2ParserListener) EnterSchema_clause(ctx *Schema_clauseContext) {}

// ExitSchema_clause is called when production schema_clause is exited.
func (s *BaseDb2ParserListener) ExitSchema_clause(ctx *Schema_clauseContext) {}

// EnterCreate_workload_statement is called when production create_workload_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_workload_statement(ctx *Create_workload_statementContext) {
}

// ExitCreate_workload_statement is called when production create_workload_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_workload_statement(ctx *Create_workload_statementContext) {
}

// EnterPkg_exec_seq is called when production pkg_exec_seq is entered.
func (s *BaseDb2ParserListener) EnterPkg_exec_seq(ctx *Pkg_exec_seqContext) {}

// ExitPkg_exec_seq is called when production pkg_exec_seq is exited.
func (s *BaseDb2ParserListener) ExitPkg_exec_seq(ctx *Pkg_exec_seqContext) {}

// EnterPosition_clause_2 is called when production position_clause_2 is entered.
func (s *BaseDb2ParserListener) EnterPosition_clause_2(ctx *Position_clause_2Context) {}

// ExitPosition_clause_2 is called when production position_clause_2 is exited.
func (s *BaseDb2ParserListener) ExitPosition_clause_2(ctx *Position_clause_2Context) {}

// EnterConnection_attributes is called when production connection_attributes is entered.
func (s *BaseDb2ParserListener) EnterConnection_attributes(ctx *Connection_attributesContext) {}

// ExitConnection_attributes is called when production connection_attributes is exited.
func (s *BaseDb2ParserListener) ExitConnection_attributes(ctx *Connection_attributesContext) {}

// EnterString_list is called when production string_list is entered.
func (s *BaseDb2ParserListener) EnterString_list(ctx *String_listContext) {}

// ExitString_list is called when production string_list is exited.
func (s *BaseDb2ParserListener) ExitString_list(ctx *String_listContext) {}

// EnterString_list_paren is called when production string_list_paren is entered.
func (s *BaseDb2ParserListener) EnterString_list_paren(ctx *String_list_parenContext) {}

// ExitString_list_paren is called when production string_list_paren is exited.
func (s *BaseDb2ParserListener) ExitString_list_paren(ctx *String_list_parenContext) {}

// EnterWorkload_attributes is called when production workload_attributes is entered.
func (s *BaseDb2ParserListener) EnterWorkload_attributes(ctx *Workload_attributesContext) {}

// ExitWorkload_attributes is called when production workload_attributes is exited.
func (s *BaseDb2ParserListener) ExitWorkload_attributes(ctx *Workload_attributesContext) {}

// EnterDegree is called when production degree is entered.
func (s *BaseDb2ParserListener) EnterDegree(ctx *DegreeContext) {}

// ExitDegree is called when production degree is exited.
func (s *BaseDb2ParserListener) ExitDegree(ctx *DegreeContext) {}

// EnterAllow_disallow is called when production allow_disallow is entered.
func (s *BaseDb2ParserListener) EnterAllow_disallow(ctx *Allow_disallowContext) {}

// ExitAllow_disallow is called when production allow_disallow is exited.
func (s *BaseDb2ParserListener) ExitAllow_disallow(ctx *Allow_disallowContext) {}

// EnterCollect_on_clause is called when production collect_on_clause is entered.
func (s *BaseDb2ParserListener) EnterCollect_on_clause(ctx *Collect_on_clauseContext) {}

// ExitCollect_on_clause is called when production collect_on_clause is exited.
func (s *BaseDb2ParserListener) ExitCollect_on_clause(ctx *Collect_on_clauseContext) {}

// EnterCollect_details_clause is called when production collect_details_clause is entered.
func (s *BaseDb2ParserListener) EnterCollect_details_clause(ctx *Collect_details_clauseContext) {}

// ExitCollect_details_clause is called when production collect_details_clause is exited.
func (s *BaseDb2ParserListener) ExitCollect_details_clause(ctx *Collect_details_clauseContext) {}

// EnterCollect_lock_wait_options is called when production collect_lock_wait_options is entered.
func (s *BaseDb2ParserListener) EnterCollect_lock_wait_options(ctx *Collect_lock_wait_optionsContext) {
}

// ExitCollect_lock_wait_options is called when production collect_lock_wait_options is exited.
func (s *BaseDb2ParserListener) ExitCollect_lock_wait_options(ctx *Collect_lock_wait_optionsContext) {
}

// EnterWait_time is called when production wait_time is entered.
func (s *BaseDb2ParserListener) EnterWait_time(ctx *Wait_timeContext) {}

// ExitWait_time is called when production wait_time is exited.
func (s *BaseDb2ParserListener) ExitWait_time(ctx *Wait_timeContext) {}

// EnterCreate_wrapper_statement is called when production create_wrapper_statement is entered.
func (s *BaseDb2ParserListener) EnterCreate_wrapper_statement(ctx *Create_wrapper_statementContext) {}

// ExitCreate_wrapper_statement is called when production create_wrapper_statement is exited.
func (s *BaseDb2ParserListener) ExitCreate_wrapper_statement(ctx *Create_wrapper_statementContext) {}

// EnterWrapper_option_list is called when production wrapper_option_list is entered.
func (s *BaseDb2ParserListener) EnterWrapper_option_list(ctx *Wrapper_option_listContext) {}

// ExitWrapper_option_list is called when production wrapper_option_list is exited.
func (s *BaseDb2ParserListener) ExitWrapper_option_list(ctx *Wrapper_option_listContext) {}

// EnterWrapper_option is called when production wrapper_option is entered.
func (s *BaseDb2ParserListener) EnterWrapper_option(ctx *Wrapper_optionContext) {}

// ExitWrapper_option is called when production wrapper_option is exited.
func (s *BaseDb2ParserListener) ExitWrapper_option(ctx *Wrapper_optionContext) {}

// EnterExpression is called when production expression is entered.
func (s *BaseDb2ParserListener) EnterExpression(ctx *ExpressionContext) {}

// ExitExpression is called when production expression is exited.
func (s *BaseDb2ParserListener) ExitExpression(ctx *ExpressionContext) {}

// EnterFunction_invocation is called when production function_invocation is entered.
func (s *BaseDb2ParserListener) EnterFunction_invocation(ctx *Function_invocationContext) {}

// ExitFunction_invocation is called when production function_invocation is exited.
func (s *BaseDb2ParserListener) ExitFunction_invocation(ctx *Function_invocationContext) {}

// EnterAll_distinct is called when production all_distinct is entered.
func (s *BaseDb2ParserListener) EnterAll_distinct(ctx *All_distinctContext) {}

// ExitAll_distinct is called when production all_distinct is exited.
func (s *BaseDb2ParserListener) ExitAll_distinct(ctx *All_distinctContext) {}

// EnterScalar_fullselect is called when production scalar_fullselect is entered.
func (s *BaseDb2ParserListener) EnterScalar_fullselect(ctx *Scalar_fullselectContext) {}

// ExitScalar_fullselect is called when production scalar_fullselect is exited.
func (s *BaseDb2ParserListener) ExitScalar_fullselect(ctx *Scalar_fullselectContext) {}

// EnterCast_specification is called when production cast_specification is entered.
func (s *BaseDb2ParserListener) EnterCast_specification(ctx *Cast_specificationContext) {}

// ExitCast_specification is called when production cast_specification is exited.
func (s *BaseDb2ParserListener) ExitCast_specification(ctx *Cast_specificationContext) {}

// EnterCursor_cast_specification is called when production cursor_cast_specification is entered.
func (s *BaseDb2ParserListener) EnterCursor_cast_specification(ctx *Cursor_cast_specificationContext) {
}

// ExitCursor_cast_specification is called when production cursor_cast_specification is exited.
func (s *BaseDb2ParserListener) ExitCursor_cast_specification(ctx *Cursor_cast_specificationContext) {
}

// EnterRow_cast_specification is called when production row_cast_specification is entered.
func (s *BaseDb2ParserListener) EnterRow_cast_specification(ctx *Row_cast_specificationContext) {}

// ExitRow_cast_specification is called when production row_cast_specification is exited.
func (s *BaseDb2ParserListener) ExitRow_cast_specification(ctx *Row_cast_specificationContext) {}

// EnterInterval_cast_specification is called when production interval_cast_specification is entered.
func (s *BaseDb2ParserListener) EnterInterval_cast_specification(ctx *Interval_cast_specificationContext) {
}

// ExitInterval_cast_specification is called when production interval_cast_specification is exited.
func (s *BaseDb2ParserListener) ExitInterval_cast_specification(ctx *Interval_cast_specificationContext) {
}

// EnterXmlcast_specification is called when production xmlcast_specification is entered.
func (s *BaseDb2ParserListener) EnterXmlcast_specification(ctx *Xmlcast_specificationContext) {}

// ExitXmlcast_specification is called when production xmlcast_specification is exited.
func (s *BaseDb2ParserListener) ExitXmlcast_specification(ctx *Xmlcast_specificationContext) {}

// EnterArray_element_specification is called when production array_element_specification is entered.
func (s *BaseDb2ParserListener) EnterArray_element_specification(ctx *Array_element_specificationContext) {
}

// ExitArray_element_specification is called when production array_element_specification is exited.
func (s *BaseDb2ParserListener) ExitArray_element_specification(ctx *Array_element_specificationContext) {
}

// EnterArray_constructor is called when production array_constructor is entered.
func (s *BaseDb2ParserListener) EnterArray_constructor(ctx *Array_constructorContext) {}

// ExitArray_constructor is called when production array_constructor is exited.
func (s *BaseDb2ParserListener) ExitArray_constructor(ctx *Array_constructorContext) {}

// EnterMethod_invocation is called when production method_invocation is entered.
func (s *BaseDb2ParserListener) EnterMethod_invocation(ctx *Method_invocationContext) {}

// ExitMethod_invocation is called when production method_invocation is exited.
func (s *BaseDb2ParserListener) ExitMethod_invocation(ctx *Method_invocationContext) {}

// EnterOlap_specification is called when production olap_specification is entered.
func (s *BaseDb2ParserListener) EnterOlap_specification(ctx *Olap_specificationContext) {}

// ExitOlap_specification is called when production olap_specification is exited.
func (s *BaseDb2ParserListener) ExitOlap_specification(ctx *Olap_specificationContext) {}

// EnterOrdered_olap_specification is called when production ordered_olap_specification is entered.
func (s *BaseDb2ParserListener) EnterOrdered_olap_specification(ctx *Ordered_olap_specificationContext) {
}

// ExitOrdered_olap_specification is called when production ordered_olap_specification is exited.
func (s *BaseDb2ParserListener) ExitOrdered_olap_specification(ctx *Ordered_olap_specificationContext) {
}

// EnterWindow_partition_clause is called when production window_partition_clause is entered.
func (s *BaseDb2ParserListener) EnterWindow_partition_clause(ctx *Window_partition_clauseContext) {}

// ExitWindow_partition_clause is called when production window_partition_clause is exited.
func (s *BaseDb2ParserListener) ExitWindow_partition_clause(ctx *Window_partition_clauseContext) {}

// EnterWindow_order_clause is called when production window_order_clause is entered.
func (s *BaseDb2ParserListener) EnterWindow_order_clause(ctx *Window_order_clauseContext) {}

// ExitWindow_order_clause is called when production window_order_clause is exited.
func (s *BaseDb2ParserListener) ExitWindow_order_clause(ctx *Window_order_clauseContext) {}

// EnterNumbering_specification is called when production numbering_specification is entered.
func (s *BaseDb2ParserListener) EnterNumbering_specification(ctx *Numbering_specificationContext) {}

// ExitNumbering_specification is called when production numbering_specification is exited.
func (s *BaseDb2ParserListener) ExitNumbering_specification(ctx *Numbering_specificationContext) {}

// EnterAggregation_specification is called when production aggregation_specification is entered.
func (s *BaseDb2ParserListener) EnterAggregation_specification(ctx *Aggregation_specificationContext) {
}

// ExitAggregation_specification is called when production aggregation_specification is exited.
func (s *BaseDb2ParserListener) ExitAggregation_specification(ctx *Aggregation_specificationContext) {
}

// EnterOlap_aggregate_function is called when production olap_aggregate_function is entered.
func (s *BaseDb2ParserListener) EnterOlap_aggregate_function(ctx *Olap_aggregate_functionContext) {}

// ExitOlap_aggregate_function is called when production olap_aggregate_function is exited.
func (s *BaseDb2ParserListener) ExitOlap_aggregate_function(ctx *Olap_aggregate_functionContext) {}

// EnterFirst_value_function is called when production first_value_function is entered.
func (s *BaseDb2ParserListener) EnterFirst_value_function(ctx *First_value_functionContext) {}

// ExitFirst_value_function is called when production first_value_function is exited.
func (s *BaseDb2ParserListener) ExitFirst_value_function(ctx *First_value_functionContext) {}

// EnterLast_value_function is called when production last_value_function is entered.
func (s *BaseDb2ParserListener) EnterLast_value_function(ctx *Last_value_functionContext) {}

// ExitLast_value_function is called when production last_value_function is exited.
func (s *BaseDb2ParserListener) ExitLast_value_function(ctx *Last_value_functionContext) {}

// EnterNth_value_function is called when production nth_value_function is entered.
func (s *BaseDb2ParserListener) EnterNth_value_function(ctx *Nth_value_functionContext) {}

// ExitNth_value_function is called when production nth_value_function is exited.
func (s *BaseDb2ParserListener) ExitNth_value_function(ctx *Nth_value_functionContext) {}

// EnterRatio_to_report_function is called when production ratio_to_report_function is entered.
func (s *BaseDb2ParserListener) EnterRatio_to_report_function(ctx *Ratio_to_report_functionContext) {}

// ExitRatio_to_report_function is called when production ratio_to_report_function is exited.
func (s *BaseDb2ParserListener) ExitRatio_to_report_function(ctx *Ratio_to_report_functionContext) {}

// EnterIgnore_respect_nulls is called when production ignore_respect_nulls is entered.
func (s *BaseDb2ParserListener) EnterIgnore_respect_nulls(ctx *Ignore_respect_nullsContext) {}

// ExitIgnore_respect_nulls is called when production ignore_respect_nulls is exited.
func (s *BaseDb2ParserListener) ExitIgnore_respect_nulls(ctx *Ignore_respect_nullsContext) {}

// EnterFrom_first_last is called when production from_first_last is entered.
func (s *BaseDb2ParserListener) EnterFrom_first_last(ctx *From_first_lastContext) {}

// ExitFrom_first_last is called when production from_first_last is exited.
func (s *BaseDb2ParserListener) ExitFrom_first_last(ctx *From_first_lastContext) {}

// EnterWindow_aggregation_group_clause is called when production window_aggregation_group_clause is entered.
func (s *BaseDb2ParserListener) EnterWindow_aggregation_group_clause(ctx *Window_aggregation_group_clauseContext) {
}

// ExitWindow_aggregation_group_clause is called when production window_aggregation_group_clause is exited.
func (s *BaseDb2ParserListener) ExitWindow_aggregation_group_clause(ctx *Window_aggregation_group_clauseContext) {
}

// EnterGroup_start is called when production group_start is entered.
func (s *BaseDb2ParserListener) EnterGroup_start(ctx *Group_startContext) {}

// ExitGroup_start is called when production group_start is exited.
func (s *BaseDb2ParserListener) ExitGroup_start(ctx *Group_startContext) {}

// EnterGroup_between is called when production group_between is entered.
func (s *BaseDb2ParserListener) EnterGroup_between(ctx *Group_betweenContext) {}

// ExitGroup_between is called when production group_between is exited.
func (s *BaseDb2ParserListener) ExitGroup_between(ctx *Group_betweenContext) {}

// EnterGroup_bound1 is called when production group_bound1 is entered.
func (s *BaseDb2ParserListener) EnterGroup_bound1(ctx *Group_bound1Context) {}

// ExitGroup_bound1 is called when production group_bound1 is exited.
func (s *BaseDb2ParserListener) ExitGroup_bound1(ctx *Group_bound1Context) {}

// EnterGroup_bound2 is called when production group_bound2 is entered.
func (s *BaseDb2ParserListener) EnterGroup_bound2(ctx *Group_bound2Context) {}

// ExitGroup_bound2 is called when production group_bound2 is exited.
func (s *BaseDb2ParserListener) ExitGroup_bound2(ctx *Group_bound2Context) {}

// EnterGroup_end is called when production group_end is entered.
func (s *BaseDb2ParserListener) EnterGroup_end(ctx *Group_endContext) {}

// ExitGroup_end is called when production group_end is exited.
func (s *BaseDb2ParserListener) ExitGroup_end(ctx *Group_endContext) {}

// EnterRow_change_expression is called when production row_change_expression is entered.
func (s *BaseDb2ParserListener) EnterRow_change_expression(ctx *Row_change_expressionContext) {}

// ExitRow_change_expression is called when production row_change_expression is exited.
func (s *BaseDb2ParserListener) ExitRow_change_expression(ctx *Row_change_expressionContext) {}

// EnterSequence_reference is called when production sequence_reference is entered.
func (s *BaseDb2ParserListener) EnterSequence_reference(ctx *Sequence_referenceContext) {}

// ExitSequence_reference is called when production sequence_reference is exited.
func (s *BaseDb2ParserListener) ExitSequence_reference(ctx *Sequence_referenceContext) {}

// EnterSubtype_treatment is called when production subtype_treatment is entered.
func (s *BaseDb2ParserListener) EnterSubtype_treatment(ctx *Subtype_treatmentContext) {}

// ExitSubtype_treatment is called when production subtype_treatment is exited.
func (s *BaseDb2ParserListener) ExitSubtype_treatment(ctx *Subtype_treatmentContext) {}

// EnterExpression_list is called when production expression_list is entered.
func (s *BaseDb2ParserListener) EnterExpression_list(ctx *Expression_listContext) {}

// ExitExpression_list is called when production expression_list is exited.
func (s *BaseDb2ParserListener) ExitExpression_list(ctx *Expression_listContext) {}

// EnterExpression_list_in_parentheses is called when production expression_list_in_parentheses is entered.
func (s *BaseDb2ParserListener) EnterExpression_list_in_parentheses(ctx *Expression_list_in_parenthesesContext) {
}

// ExitExpression_list_in_parentheses is called when production expression_list_in_parentheses is exited.
func (s *BaseDb2ParserListener) ExitExpression_list_in_parentheses(ctx *Expression_list_in_parenthesesContext) {
}

// EnterId_ is called when production id_ is entered.
func (s *BaseDb2ParserListener) EnterId_(ctx *Id_Context) {}

// ExitId_ is called when production id_ is exited.
func (s *BaseDb2ParserListener) ExitId_(ctx *Id_Context) {}

// EnterExposed_name is called when production exposed_name is entered.
func (s *BaseDb2ParserListener) EnterExposed_name(ctx *Exposed_nameContext) {}

// ExitExposed_name is called when production exposed_name is exited.
func (s *BaseDb2ParserListener) ExitExposed_name(ctx *Exposed_nameContext) {}

// EnterName is called when production name is entered.
func (s *BaseDb2ParserListener) EnterName(ctx *NameContext) {}

// ExitName is called when production name is exited.
func (s *BaseDb2ParserListener) ExitName(ctx *NameContext) {}

// EnterLabel is called when production label is entered.
func (s *BaseDb2ParserListener) EnterLabel(ctx *LabelContext) {}

// ExitLabel is called when production label is exited.
func (s *BaseDb2ParserListener) ExitLabel(ctx *LabelContext) {}

// EnterHost_label is called when production host_label is entered.
func (s *BaseDb2ParserListener) EnterHost_label(ctx *Host_labelContext) {}

// ExitHost_label is called when production host_label is exited.
func (s *BaseDb2ParserListener) ExitHost_label(ctx *Host_labelContext) {}

// EnterLibrary_name is called when production library_name is entered.
func (s *BaseDb2ParserListener) EnterLibrary_name(ctx *Library_nameContext) {}

// ExitLibrary_name is called when production library_name is exited.
func (s *BaseDb2ParserListener) ExitLibrary_name(ctx *Library_nameContext) {}

// EnterArray_type_name is called when production array_type_name is entered.
func (s *BaseDb2ParserListener) EnterArray_type_name(ctx *Array_type_nameContext) {}

// ExitArray_type_name is called when production array_type_name is exited.
func (s *BaseDb2ParserListener) ExitArray_type_name(ctx *Array_type_nameContext) {}

// EnterAttribute_name is called when production attribute_name is entered.
func (s *BaseDb2ParserListener) EnterAttribute_name(ctx *Attribute_nameContext) {}

// ExitAttribute_name is called when production attribute_name is exited.
func (s *BaseDb2ParserListener) ExitAttribute_name(ctx *Attribute_nameContext) {}

// EnterRow_type_name is called when production row_type_name is entered.
func (s *BaseDb2ParserListener) EnterRow_type_name(ctx *Row_type_nameContext) {}

// ExitRow_type_name is called when production row_type_name is exited.
func (s *BaseDb2ParserListener) ExitRow_type_name(ctx *Row_type_nameContext) {}

// EnterAuthorization_name is called when production authorization_name is entered.
func (s *BaseDb2ParserListener) EnterAuthorization_name(ctx *Authorization_nameContext) {}

// ExitAuthorization_name is called when production authorization_name is exited.
func (s *BaseDb2ParserListener) ExitAuthorization_name(ctx *Authorization_nameContext) {}

// EnterBoolean_variable_name is called when production boolean_variable_name is entered.
func (s *BaseDb2ParserListener) EnterBoolean_variable_name(ctx *Boolean_variable_nameContext) {}

// ExitBoolean_variable_name is called when production boolean_variable_name is exited.
func (s *BaseDb2ParserListener) ExitBoolean_variable_name(ctx *Boolean_variable_nameContext) {}

// EnterArray_variable_name is called when production array_variable_name is entered.
func (s *BaseDb2ParserListener) EnterArray_variable_name(ctx *Array_variable_nameContext) {}

// ExitArray_variable_name is called when production array_variable_name is exited.
func (s *BaseDb2ParserListener) ExitArray_variable_name(ctx *Array_variable_nameContext) {}

// EnterColumn_name is called when production column_name is entered.
func (s *BaseDb2ParserListener) EnterColumn_name(ctx *Column_nameContext) {}

// ExitColumn_name is called when production column_name is exited.
func (s *BaseDb2ParserListener) ExitColumn_name(ctx *Column_nameContext) {}

// EnterConstraint_name is called when production constraint_name is entered.
func (s *BaseDb2ParserListener) EnterConstraint_name(ctx *Constraint_nameContext) {}

// ExitConstraint_name is called when production constraint_name is exited.
func (s *BaseDb2ParserListener) ExitConstraint_name(ctx *Constraint_nameContext) {}

// EnterDescriptor_name is called when production descriptor_name is entered.
func (s *BaseDb2ParserListener) EnterDescriptor_name(ctx *Descriptor_nameContext) {}

// ExitDescriptor_name is called when production descriptor_name is exited.
func (s *BaseDb2ParserListener) ExitDescriptor_name(ctx *Descriptor_nameContext) {}

// EnterDistinct_type_name is called when production distinct_type_name is entered.
func (s *BaseDb2ParserListener) EnterDistinct_type_name(ctx *Distinct_type_nameContext) {}

// ExitDistinct_type_name is called when production distinct_type_name is exited.
func (s *BaseDb2ParserListener) ExitDistinct_type_name(ctx *Distinct_type_nameContext) {}

// EnterCursor_name is called when production cursor_name is entered.
func (s *BaseDb2ParserListener) EnterCursor_name(ctx *Cursor_nameContext) {}

// ExitCursor_name is called when production cursor_name is exited.
func (s *BaseDb2ParserListener) ExitCursor_name(ctx *Cursor_nameContext) {}

// EnterCursor_type_name is called when production cursor_type_name is entered.
func (s *BaseDb2ParserListener) EnterCursor_type_name(ctx *Cursor_type_nameContext) {}

// ExitCursor_type_name is called when production cursor_type_name is exited.
func (s *BaseDb2ParserListener) ExitCursor_type_name(ctx *Cursor_type_nameContext) {}

// EnterCondition_name is called when production condition_name is entered.
func (s *BaseDb2ParserListener) EnterCondition_name(ctx *Condition_nameContext) {}

// ExitCondition_name is called when production condition_name is exited.
func (s *BaseDb2ParserListener) ExitCondition_name(ctx *Condition_nameContext) {}

// EnterData_source_name is called when production data_source_name is entered.
func (s *BaseDb2ParserListener) EnterData_source_name(ctx *Data_source_nameContext) {}

// ExitData_source_name is called when production data_source_name is exited.
func (s *BaseDb2ParserListener) ExitData_source_name(ctx *Data_source_nameContext) {}

// EnterExpression_name is called when production expression_name is entered.
func (s *BaseDb2ParserListener) EnterExpression_name(ctx *Expression_nameContext) {}

// ExitExpression_name is called when production expression_name is exited.
func (s *BaseDb2ParserListener) ExitExpression_name(ctx *Expression_nameContext) {}

// EnterGroup_name is called when production group_name is entered.
func (s *BaseDb2ParserListener) EnterGroup_name(ctx *Group_nameContext) {}

// ExitGroup_name is called when production group_name is exited.
func (s *BaseDb2ParserListener) ExitGroup_name(ctx *Group_nameContext) {}

// EnterPolicy_name is called when production policy_name is entered.
func (s *BaseDb2ParserListener) EnterPolicy_name(ctx *Policy_nameContext) {}

// ExitPolicy_name is called when production policy_name is exited.
func (s *BaseDb2ParserListener) ExitPolicy_name(ctx *Policy_nameContext) {}

// EnterBufferpool_name is called when production bufferpool_name is entered.
func (s *BaseDb2ParserListener) EnterBufferpool_name(ctx *Bufferpool_nameContext) {}

// ExitBufferpool_name is called when production bufferpool_name is exited.
func (s *BaseDb2ParserListener) ExitBufferpool_name(ctx *Bufferpool_nameContext) {}

// EnterDb_partition_name is called when production db_partition_name is entered.
func (s *BaseDb2ParserListener) EnterDb_partition_name(ctx *Db_partition_nameContext) {}

// ExitDb_partition_name is called when production db_partition_name is exited.
func (s *BaseDb2ParserListener) ExitDb_partition_name(ctx *Db_partition_nameContext) {}

// EnterDatabase_name is called when production database_name is entered.
func (s *BaseDb2ParserListener) EnterDatabase_name(ctx *Database_nameContext) {}

// ExitDatabase_name is called when production database_name is exited.
func (s *BaseDb2ParserListener) ExitDatabase_name(ctx *Database_nameContext) {}

// EnterEvent_monitor_name is called when production event_monitor_name is entered.
func (s *BaseDb2ParserListener) EnterEvent_monitor_name(ctx *Event_monitor_nameContext) {}

// ExitEvent_monitor_name is called when production event_monitor_name is exited.
func (s *BaseDb2ParserListener) ExitEvent_monitor_name(ctx *Event_monitor_nameContext) {}

// EnterField_name is called when production field_name is entered.
func (s *BaseDb2ParserListener) EnterField_name(ctx *Field_nameContext) {}

// ExitField_name is called when production field_name is exited.
func (s *BaseDb2ParserListener) ExitField_name(ctx *Field_nameContext) {}

// EnterFor_loop_name is called when production for_loop_name is entered.
func (s *BaseDb2ParserListener) EnterFor_loop_name(ctx *For_loop_nameContext) {}

// ExitFor_loop_name is called when production for_loop_name is exited.
func (s *BaseDb2ParserListener) ExitFor_loop_name(ctx *For_loop_nameContext) {}

// EnterFunction_name is called when production function_name is entered.
func (s *BaseDb2ParserListener) EnterFunction_name(ctx *Function_nameContext) {}

// ExitFunction_name is called when production function_name is exited.
func (s *BaseDb2ParserListener) ExitFunction_name(ctx *Function_nameContext) {}

// EnterFunction_mapping_name is called when production function_mapping_name is entered.
func (s *BaseDb2ParserListener) EnterFunction_mapping_name(ctx *Function_mapping_nameContext) {}

// ExitFunction_mapping_name is called when production function_mapping_name is exited.
func (s *BaseDb2ParserListener) ExitFunction_mapping_name(ctx *Function_mapping_nameContext) {}

// EnterGlobal_variable_name is called when production global_variable_name is entered.
func (s *BaseDb2ParserListener) EnterGlobal_variable_name(ctx *Global_variable_nameContext) {}

// ExitGlobal_variable_name is called when production global_variable_name is exited.
func (s *BaseDb2ParserListener) ExitGlobal_variable_name(ctx *Global_variable_nameContext) {}

// EnterHierarchy_name is called when production hierarchy_name is entered.
func (s *BaseDb2ParserListener) EnterHierarchy_name(ctx *Hierarchy_nameContext) {}

// ExitHierarchy_name is called when production hierarchy_name is exited.
func (s *BaseDb2ParserListener) ExitHierarchy_name(ctx *Hierarchy_nameContext) {}

// EnterHost_variable_name is called when production host_variable_name is entered.
func (s *BaseDb2ParserListener) EnterHost_variable_name(ctx *Host_variable_nameContext) {}

// ExitHost_variable_name is called when production host_variable_name is exited.
func (s *BaseDb2ParserListener) ExitHost_variable_name(ctx *Host_variable_nameContext) {}

// EnterParameter_marker is called when production parameter_marker is entered.
func (s *BaseDb2ParserListener) EnterParameter_marker(ctx *Parameter_markerContext) {}

// ExitParameter_marker is called when production parameter_marker is exited.
func (s *BaseDb2ParserListener) ExitParameter_marker(ctx *Parameter_markerContext) {}

// EnterTemplate_name is called when production template_name is entered.
func (s *BaseDb2ParserListener) EnterTemplate_name(ctx *Template_nameContext) {}

// ExitTemplate_name is called when production template_name is exited.
func (s *BaseDb2ParserListener) ExitTemplate_name(ctx *Template_nameContext) {}

// EnterIndex_name is called when production index_name is entered.
func (s *BaseDb2ParserListener) EnterIndex_name(ctx *Index_nameContext) {}

// ExitIndex_name is called when production index_name is exited.
func (s *BaseDb2ParserListener) ExitIndex_name(ctx *Index_nameContext) {}

// EnterIndex_extension_name is called when production index_extension_name is entered.
func (s *BaseDb2ParserListener) EnterIndex_extension_name(ctx *Index_extension_nameContext) {}

// ExitIndex_extension_name is called when production index_extension_name is exited.
func (s *BaseDb2ParserListener) ExitIndex_extension_name(ctx *Index_extension_nameContext) {}

// EnterInput_descriptor_name is called when production input_descriptor_name is entered.
func (s *BaseDb2ParserListener) EnterInput_descriptor_name(ctx *Input_descriptor_nameContext) {}

// ExitInput_descriptor_name is called when production input_descriptor_name is exited.
func (s *BaseDb2ParserListener) ExitInput_descriptor_name(ctx *Input_descriptor_nameContext) {}

// EnterMask_name is called when production mask_name is entered.
func (s *BaseDb2ParserListener) EnterMask_name(ctx *Mask_nameContext) {}

// ExitMask_name is called when production mask_name is exited.
func (s *BaseDb2ParserListener) ExitMask_name(ctx *Mask_nameContext) {}

// EnterMethod_name is called when production method_name is entered.
func (s *BaseDb2ParserListener) EnterMethod_name(ctx *Method_nameContext) {}

// ExitMethod_name is called when production method_name is exited.
func (s *BaseDb2ParserListener) ExitMethod_name(ctx *Method_nameContext) {}

// EnterModel_name is called when production model_name is entered.
func (s *BaseDb2ParserListener) EnterModel_name(ctx *Model_nameContext) {}

// ExitModel_name is called when production model_name is exited.
func (s *BaseDb2ParserListener) ExitModel_name(ctx *Model_nameContext) {}

// EnterModule_name is called when production module_name is entered.
func (s *BaseDb2ParserListener) EnterModule_name(ctx *Module_nameContext) {}

// ExitModule_name is called when production module_name is exited.
func (s *BaseDb2ParserListener) ExitModule_name(ctx *Module_nameContext) {}

// EnterNew_owner is called when production new_owner is entered.
func (s *BaseDb2ParserListener) EnterNew_owner(ctx *New_ownerContext) {}

// ExitNew_owner is called when production new_owner is exited.
func (s *BaseDb2ParserListener) ExitNew_owner(ctx *New_ownerContext) {}

// EnterNick_name is called when production nick_name is entered.
func (s *BaseDb2ParserListener) EnterNick_name(ctx *Nick_nameContext) {}

// ExitNick_name is called when production nick_name is exited.
func (s *BaseDb2ParserListener) ExitNick_name(ctx *Nick_nameContext) {}

// EnterObject_name is called when production object_name is entered.
func (s *BaseDb2ParserListener) EnterObject_name(ctx *Object_nameContext) {}

// ExitObject_name is called when production object_name is exited.
func (s *BaseDb2ParserListener) ExitObject_name(ctx *Object_nameContext) {}

// EnterOid_column_name is called when production oid_column_name is entered.
func (s *BaseDb2ParserListener) EnterOid_column_name(ctx *Oid_column_nameContext) {}

// ExitOid_column_name is called when production oid_column_name is exited.
func (s *BaseDb2ParserListener) ExitOid_column_name(ctx *Oid_column_nameContext) {}

// EnterOptimization_profile_name is called when production optimization_profile_name is entered.
func (s *BaseDb2ParserListener) EnterOptimization_profile_name(ctx *Optimization_profile_nameContext) {
}

// ExitOptimization_profile_name is called when production optimization_profile_name is exited.
func (s *BaseDb2ParserListener) ExitOptimization_profile_name(ctx *Optimization_profile_nameContext) {
}

// EnterPackage_name is called when production package_name is entered.
func (s *BaseDb2ParserListener) EnterPackage_name(ctx *Package_nameContext) {}

// ExitPackage_name is called when production package_name is exited.
func (s *BaseDb2ParserListener) ExitPackage_name(ctx *Package_nameContext) {}

// EnterPartition_name is called when production partition_name is entered.
func (s *BaseDb2ParserListener) EnterPartition_name(ctx *Partition_nameContext) {}

// ExitPartition_name is called when production partition_name is exited.
func (s *BaseDb2ParserListener) ExitPartition_name(ctx *Partition_nameContext) {}

// EnterPath_name is called when production path_name is entered.
func (s *BaseDb2ParserListener) EnterPath_name(ctx *Path_nameContext) {}

// ExitPath_name is called when production path_name is exited.
func (s *BaseDb2ParserListener) ExitPath_name(ctx *Path_nameContext) {}

// EnterPermission_name is called when production permission_name is entered.
func (s *BaseDb2ParserListener) EnterPermission_name(ctx *Permission_nameContext) {}

// ExitPermission_name is called when production permission_name is exited.
func (s *BaseDb2ParserListener) ExitPermission_name(ctx *Permission_nameContext) {}

// EnterPipe_name is called when production pipe_name is entered.
func (s *BaseDb2ParserListener) EnterPipe_name(ctx *Pipe_nameContext) {}

// ExitPipe_name is called when production pipe_name is exited.
func (s *BaseDb2ParserListener) ExitPipe_name(ctx *Pipe_nameContext) {}

// EnterProcedure_name is called when production procedure_name is entered.
func (s *BaseDb2ParserListener) EnterProcedure_name(ctx *Procedure_nameContext) {}

// ExitProcedure_name is called when production procedure_name is exited.
func (s *BaseDb2ParserListener) ExitProcedure_name(ctx *Procedure_nameContext) {}

// EnterResult_descriptor_name is called when production result_descriptor_name is entered.
func (s *BaseDb2ParserListener) EnterResult_descriptor_name(ctx *Result_descriptor_nameContext) {}

// ExitResult_descriptor_name is called when production result_descriptor_name is exited.
func (s *BaseDb2ParserListener) ExitResult_descriptor_name(ctx *Result_descriptor_nameContext) {}

// EnterRole_name is called when production role_name is entered.
func (s *BaseDb2ParserListener) EnterRole_name(ctx *Role_nameContext) {}

// ExitRole_name is called when production role_name is exited.
func (s *BaseDb2ParserListener) ExitRole_name(ctx *Role_nameContext) {}

// EnterRoot_table_name is called when production root_table_name is entered.
func (s *BaseDb2ParserListener) EnterRoot_table_name(ctx *Root_table_nameContext) {}

// ExitRoot_table_name is called when production root_table_name is exited.
func (s *BaseDb2ParserListener) ExitRoot_table_name(ctx *Root_table_nameContext) {}

// EnterRoot_view_name is called when production root_view_name is entered.
func (s *BaseDb2ParserListener) EnterRoot_view_name(ctx *Root_view_nameContext) {}

// ExitRoot_view_name is called when production root_view_name is exited.
func (s *BaseDb2ParserListener) ExitRoot_view_name(ctx *Root_view_nameContext) {}

// EnterRow_variable_name is called when production row_variable_name is entered.
func (s *BaseDb2ParserListener) EnterRow_variable_name(ctx *Row_variable_nameContext) {}

// ExitRow_variable_name is called when production row_variable_name is exited.
func (s *BaseDb2ParserListener) ExitRow_variable_name(ctx *Row_variable_nameContext) {}

// EnterSource_schema_name is called when production source_schema_name is entered.
func (s *BaseDb2ParserListener) EnterSource_schema_name(ctx *Source_schema_nameContext) {}

// ExitSource_schema_name is called when production source_schema_name is exited.
func (s *BaseDb2ParserListener) ExitSource_schema_name(ctx *Source_schema_nameContext) {}

// EnterSource_package_name is called when production source_package_name is entered.
func (s *BaseDb2ParserListener) EnterSource_package_name(ctx *Source_package_nameContext) {}

// ExitSource_package_name is called when production source_package_name is exited.
func (s *BaseDb2ParserListener) ExitSource_package_name(ctx *Source_package_nameContext) {}

// EnterSource_procedure_name is called when production source_procedure_name is entered.
func (s *BaseDb2ParserListener) EnterSource_procedure_name(ctx *Source_procedure_nameContext) {}

// ExitSource_procedure_name is called when production source_procedure_name is exited.
func (s *BaseDb2ParserListener) ExitSource_procedure_name(ctx *Source_procedure_nameContext) {}

// EnterSql_parameter_name is called when production sql_parameter_name is entered.
func (s *BaseDb2ParserListener) EnterSql_parameter_name(ctx *Sql_parameter_nameContext) {}

// ExitSql_parameter_name is called when production sql_parameter_name is exited.
func (s *BaseDb2ParserListener) ExitSql_parameter_name(ctx *Sql_parameter_nameContext) {}

// EnterSql_variable_name is called when production sql_variable_name is entered.
func (s *BaseDb2ParserListener) EnterSql_variable_name(ctx *Sql_variable_nameContext) {}

// ExitSql_variable_name is called when production sql_variable_name is exited.
func (s *BaseDb2ParserListener) ExitSql_variable_name(ctx *Sql_variable_nameContext) {}

// EnterTransition_variable_name is called when production transition_variable_name is entered.
func (s *BaseDb2ParserListener) EnterTransition_variable_name(ctx *Transition_variable_nameContext) {}

// ExitTransition_variable_name is called when production transition_variable_name is exited.
func (s *BaseDb2ParserListener) ExitTransition_variable_name(ctx *Transition_variable_nameContext) {}

// EnterSavepoint_name is called when production savepoint_name is entered.
func (s *BaseDb2ParserListener) EnterSavepoint_name(ctx *Savepoint_nameContext) {}

// ExitSavepoint_name is called when production savepoint_name is exited.
func (s *BaseDb2ParserListener) ExitSavepoint_name(ctx *Savepoint_nameContext) {}

// EnterSpecific_name is called when production specific_name is entered.
func (s *BaseDb2ParserListener) EnterSpecific_name(ctx *Specific_nameContext) {}

// ExitSpecific_name is called when production specific_name is exited.
func (s *BaseDb2ParserListener) ExitSpecific_name(ctx *Specific_nameContext) {}

// EnterSchema is called when production schema is entered.
func (s *BaseDb2ParserListener) EnterSchema(ctx *SchemaContext) {}

// ExitSchema is called when production schema is exited.
func (s *BaseDb2ParserListener) ExitSchema(ctx *SchemaContext) {}

// EnterSchema_name is called when production schema_name is entered.
func (s *BaseDb2ParserListener) EnterSchema_name(ctx *Schema_nameContext) {}

// ExitSchema_name is called when production schema_name is exited.
func (s *BaseDb2ParserListener) ExitSchema_name(ctx *Schema_nameContext) {}

// EnterSearch_method_name is called when production search_method_name is entered.
func (s *BaseDb2ParserListener) EnterSearch_method_name(ctx *Search_method_nameContext) {}

// ExitSearch_method_name is called when production search_method_name is exited.
func (s *BaseDb2ParserListener) ExitSearch_method_name(ctx *Search_method_nameContext) {}

// EnterServer_name is called when production server_name is entered.
func (s *BaseDb2ParserListener) EnterServer_name(ctx *Server_nameContext) {}

// ExitServer_name is called when production server_name is exited.
func (s *BaseDb2ParserListener) ExitServer_name(ctx *Server_nameContext) {}

// EnterServer_option_name is called when production server_option_name is entered.
func (s *BaseDb2ParserListener) EnterServer_option_name(ctx *Server_option_nameContext) {}

// ExitServer_option_name is called when production server_option_name is exited.
func (s *BaseDb2ParserListener) ExitServer_option_name(ctx *Server_option_nameContext) {}

// EnterSession_authorization_name is called when production session_authorization_name is entered.
func (s *BaseDb2ParserListener) EnterSession_authorization_name(ctx *Session_authorization_nameContext) {
}

// ExitSession_authorization_name is called when production session_authorization_name is exited.
func (s *BaseDb2ParserListener) ExitSession_authorization_name(ctx *Session_authorization_nameContext) {
}

// EnterComponent_name is called when production component_name is entered.
func (s *BaseDb2ParserListener) EnterComponent_name(ctx *Component_nameContext) {}

// ExitComponent_name is called when production component_name is exited.
func (s *BaseDb2ParserListener) ExitComponent_name(ctx *Component_nameContext) {}

// EnterSec_label_comp_name is called when production sec_label_comp_name is entered.
func (s *BaseDb2ParserListener) EnterSec_label_comp_name(ctx *Sec_label_comp_nameContext) {}

// ExitSec_label_comp_name is called when production sec_label_comp_name is exited.
func (s *BaseDb2ParserListener) ExitSec_label_comp_name(ctx *Sec_label_comp_nameContext) {}

// EnterSecurity_policy_name is called when production security_policy_name is entered.
func (s *BaseDb2ParserListener) EnterSecurity_policy_name(ctx *Security_policy_nameContext) {}

// ExitSecurity_policy_name is called when production security_policy_name is exited.
func (s *BaseDb2ParserListener) ExitSecurity_policy_name(ctx *Security_policy_nameContext) {}

// EnterSecurity_label_name is called when production security_label_name is entered.
func (s *BaseDb2ParserListener) EnterSecurity_label_name(ctx *Security_label_nameContext) {}

// ExitSecurity_label_name is called when production security_label_name is exited.
func (s *BaseDb2ParserListener) ExitSecurity_label_name(ctx *Security_label_nameContext) {}

// EnterSequence_name is called when production sequence_name is entered.
func (s *BaseDb2ParserListener) EnterSequence_name(ctx *Sequence_nameContext) {}

// ExitSequence_name is called when production sequence_name is exited.
func (s *BaseDb2ParserListener) ExitSequence_name(ctx *Sequence_nameContext) {}

// EnterService_class_name is called when production service_class_name is entered.
func (s *BaseDb2ParserListener) EnterService_class_name(ctx *Service_class_nameContext) {}

// ExitService_class_name is called when production service_class_name is exited.
func (s *BaseDb2ParserListener) ExitService_class_name(ctx *Service_class_nameContext) {}

// EnterService_superclass_name is called when production service_superclass_name is entered.
func (s *BaseDb2ParserListener) EnterService_superclass_name(ctx *Service_superclass_nameContext) {}

// ExitService_superclass_name is called when production service_superclass_name is exited.
func (s *BaseDb2ParserListener) ExitService_superclass_name(ctx *Service_superclass_nameContext) {}

// EnterStoragegroup_name is called when production storagegroup_name is entered.
func (s *BaseDb2ParserListener) EnterStoragegroup_name(ctx *Storagegroup_nameContext) {}

// ExitStoragegroup_name is called when production storagegroup_name is exited.
func (s *BaseDb2ParserListener) ExitStoragegroup_name(ctx *Storagegroup_nameContext) {}

// EnterSupertype_name is called when production supertype_name is entered.
func (s *BaseDb2ParserListener) EnterSupertype_name(ctx *Supertype_nameContext) {}

// ExitSupertype_name is called when production supertype_name is exited.
func (s *BaseDb2ParserListener) ExitSupertype_name(ctx *Supertype_nameContext) {}

// EnterSuperview_name is called when production superview_name is entered.
func (s *BaseDb2ParserListener) EnterSuperview_name(ctx *Superview_nameContext) {}

// ExitSuperview_name is called when production superview_name is exited.
func (s *BaseDb2ParserListener) ExitSuperview_name(ctx *Superview_nameContext) {}

// EnterService_subclass_name is called when production service_subclass_name is entered.
func (s *BaseDb2ParserListener) EnterService_subclass_name(ctx *Service_subclass_nameContext) {}

// ExitService_subclass_name is called when production service_subclass_name is exited.
func (s *BaseDb2ParserListener) ExitService_subclass_name(ctx *Service_subclass_nameContext) {}

// EnterStatement_name is called when production statement_name is entered.
func (s *BaseDb2ParserListener) EnterStatement_name(ctx *Statement_nameContext) {}

// ExitStatement_name is called when production statement_name is exited.
func (s *BaseDb2ParserListener) ExitStatement_name(ctx *Statement_nameContext) {}

// EnterTable_name is called when production table_name is entered.
func (s *BaseDb2ParserListener) EnterTable_name(ctx *Table_nameContext) {}

// ExitTable_name is called when production table_name is exited.
func (s *BaseDb2ParserListener) ExitTable_name(ctx *Table_nameContext) {}

// EnterTablespace_name is called when production tablespace_name is entered.
func (s *BaseDb2ParserListener) EnterTablespace_name(ctx *Tablespace_nameContext) {}

// ExitTablespace_name is called when production tablespace_name is exited.
func (s *BaseDb2ParserListener) ExitTablespace_name(ctx *Tablespace_nameContext) {}

// EnterTarget_identifier is called when production target_identifier is entered.
func (s *BaseDb2ParserListener) EnterTarget_identifier(ctx *Target_identifierContext) {}

// ExitTarget_identifier is called when production target_identifier is exited.
func (s *BaseDb2ParserListener) ExitTarget_identifier(ctx *Target_identifierContext) {}

// EnterThreshold_name is called when production threshold_name is entered.
func (s *BaseDb2ParserListener) EnterThreshold_name(ctx *Threshold_nameContext) {}

// ExitThreshold_name is called when production threshold_name is exited.
func (s *BaseDb2ParserListener) ExitThreshold_name(ctx *Threshold_nameContext) {}

// EnterTrigger_name is called when production trigger_name is entered.
func (s *BaseDb2ParserListener) EnterTrigger_name(ctx *Trigger_nameContext) {}

// ExitTrigger_name is called when production trigger_name is exited.
func (s *BaseDb2ParserListener) ExitTrigger_name(ctx *Trigger_nameContext) {}

// EnterContext_name is called when production context_name is entered.
func (s *BaseDb2ParserListener) EnterContext_name(ctx *Context_nameContext) {}

// ExitContext_name is called when production context_name is exited.
func (s *BaseDb2ParserListener) ExitContext_name(ctx *Context_nameContext) {}

// EnterUsage_list_name is called when production usage_list_name is entered.
func (s *BaseDb2ParserListener) EnterUsage_list_name(ctx *Usage_list_nameContext) {}

// ExitUsage_list_name is called when production usage_list_name is exited.
func (s *BaseDb2ParserListener) ExitUsage_list_name(ctx *Usage_list_nameContext) {}

// EnterType_name is called when production type_name is entered.
func (s *BaseDb2ParserListener) EnterType_name(ctx *Type_nameContext) {}

// ExitType_name is called when production type_name is exited.
func (s *BaseDb2ParserListener) ExitType_name(ctx *Type_nameContext) {}

// EnterType_mapping_name is called when production type_mapping_name is entered.
func (s *BaseDb2ParserListener) EnterType_mapping_name(ctx *Type_mapping_nameContext) {}

// ExitType_mapping_name is called when production type_mapping_name is exited.
func (s *BaseDb2ParserListener) ExitType_mapping_name(ctx *Type_mapping_nameContext) {}

// EnterTyped_table_name is called when production typed_table_name is entered.
func (s *BaseDb2ParserListener) EnterTyped_table_name(ctx *Typed_table_nameContext) {}

// ExitTyped_table_name is called when production typed_table_name is exited.
func (s *BaseDb2ParserListener) ExitTyped_table_name(ctx *Typed_table_nameContext) {}

// EnterTyped_view_name is called when production typed_view_name is entered.
func (s *BaseDb2ParserListener) EnterTyped_view_name(ctx *Typed_view_nameContext) {}

// ExitTyped_view_name is called when production typed_view_name is exited.
func (s *BaseDb2ParserListener) ExitTyped_view_name(ctx *Typed_view_nameContext) {}

// EnterUser_mapping_option_name is called when production user_mapping_option_name is entered.
func (s *BaseDb2ParserListener) EnterUser_mapping_option_name(ctx *User_mapping_option_nameContext) {}

// ExitUser_mapping_option_name is called when production user_mapping_option_name is exited.
func (s *BaseDb2ParserListener) ExitUser_mapping_option_name(ctx *User_mapping_option_nameContext) {}

// EnterView_name is called when production view_name is entered.
func (s *BaseDb2ParserListener) EnterView_name(ctx *View_nameContext) {}

// ExitView_name is called when production view_name is exited.
func (s *BaseDb2ParserListener) ExitView_name(ctx *View_nameContext) {}

// EnterVariable_name is called when production variable_name is entered.
func (s *BaseDb2ParserListener) EnterVariable_name(ctx *Variable_nameContext) {}

// ExitVariable_name is called when production variable_name is exited.
func (s *BaseDb2ParserListener) ExitVariable_name(ctx *Variable_nameContext) {}

// EnterWork_action_set_name is called when production work_action_set_name is entered.
func (s *BaseDb2ParserListener) EnterWork_action_set_name(ctx *Work_action_set_nameContext) {}

// ExitWork_action_set_name is called when production work_action_set_name is exited.
func (s *BaseDb2ParserListener) ExitWork_action_set_name(ctx *Work_action_set_nameContext) {}

// EnterWork_class_set_name is called when production work_class_set_name is entered.
func (s *BaseDb2ParserListener) EnterWork_class_set_name(ctx *Work_class_set_nameContext) {}

// ExitWork_class_set_name is called when production work_class_set_name is exited.
func (s *BaseDb2ParserListener) ExitWork_class_set_name(ctx *Work_class_set_nameContext) {}

// EnterWorkload_name is called when production workload_name is entered.
func (s *BaseDb2ParserListener) EnterWorkload_name(ctx *Workload_nameContext) {}

// ExitWorkload_name is called when production workload_name is exited.
func (s *BaseDb2ParserListener) ExitWorkload_name(ctx *Workload_nameContext) {}

// EnterWork_action_name is called when production work_action_name is entered.
func (s *BaseDb2ParserListener) EnterWork_action_name(ctx *Work_action_nameContext) {}

// ExitWork_action_name is called when production work_action_name is exited.
func (s *BaseDb2ParserListener) ExitWork_action_name(ctx *Work_action_nameContext) {}

// EnterWork_class_name is called when production work_class_name is entered.
func (s *BaseDb2ParserListener) EnterWork_class_name(ctx *Work_class_nameContext) {}

// ExitWork_class_name is called when production work_class_name is exited.
func (s *BaseDb2ParserListener) ExitWork_class_name(ctx *Work_class_nameContext) {}

// EnterWrapper_name is called when production wrapper_name is entered.
func (s *BaseDb2ParserListener) EnterWrapper_name(ctx *Wrapper_nameContext) {}

// ExitWrapper_name is called when production wrapper_name is exited.
func (s *BaseDb2ParserListener) ExitWrapper_name(ctx *Wrapper_nameContext) {}

// EnterWrapper_option_name is called when production wrapper_option_name is entered.
func (s *BaseDb2ParserListener) EnterWrapper_option_name(ctx *Wrapper_option_nameContext) {}

// ExitWrapper_option_name is called when production wrapper_option_name is exited.
func (s *BaseDb2ParserListener) ExitWrapper_option_name(ctx *Wrapper_option_nameContext) {}

// EnterXsrobject_name is called when production xsrobject_name is entered.
func (s *BaseDb2ParserListener) EnterXsrobject_name(ctx *Xsrobject_nameContext) {}

// ExitXsrobject_name is called when production xsrobject_name is exited.
func (s *BaseDb2ParserListener) ExitXsrobject_name(ctx *Xsrobject_nameContext) {}

// EnterParameter_name is called when production parameter_name is entered.
func (s *BaseDb2ParserListener) EnterParameter_name(ctx *Parameter_nameContext) {}

// ExitParameter_name is called when production parameter_name is exited.
func (s *BaseDb2ParserListener) ExitParameter_name(ctx *Parameter_nameContext) {}

// EnterCursor_variable_name is called when production cursor_variable_name is entered.
func (s *BaseDb2ParserListener) EnterCursor_variable_name(ctx *Cursor_variable_nameContext) {}

// ExitCursor_variable_name is called when production cursor_variable_name is exited.
func (s *BaseDb2ParserListener) ExitCursor_variable_name(ctx *Cursor_variable_nameContext) {}

// EnterAlias_name is called when production alias_name is entered.
func (s *BaseDb2ParserListener) EnterAlias_name(ctx *Alias_nameContext) {}

// ExitAlias_name is called when production alias_name is exited.
func (s *BaseDb2ParserListener) ExitAlias_name(ctx *Alias_nameContext) {}

// EnterDb_partition_group_name is called when production db_partition_group_name is entered.
func (s *BaseDb2ParserListener) EnterDb_partition_group_name(ctx *Db_partition_group_nameContext) {}

// ExitDb_partition_group_name is called when production db_partition_group_name is exited.
func (s *BaseDb2ParserListener) ExitDb_partition_group_name(ctx *Db_partition_group_nameContext) {}

// EnterSource_index_name is called when production source_index_name is entered.
func (s *BaseDb2ParserListener) EnterSource_index_name(ctx *Source_index_nameContext) {}

// ExitSource_index_name is called when production source_index_name is exited.
func (s *BaseDb2ParserListener) ExitSource_index_name(ctx *Source_index_nameContext) {}

// EnterSource_table_name is called when production source_table_name is entered.
func (s *BaseDb2ParserListener) EnterSource_table_name(ctx *Source_table_nameContext) {}

// ExitSource_table_name is called when production source_table_name is exited.
func (s *BaseDb2ParserListener) ExitSource_table_name(ctx *Source_table_nameContext) {}

// EnterSource_storagegroup_name is called when production source_storagegroup_name is entered.
func (s *BaseDb2ParserListener) EnterSource_storagegroup_name(ctx *Source_storagegroup_nameContext) {}

// ExitSource_storagegroup_name is called when production source_storagegroup_name is exited.
func (s *BaseDb2ParserListener) ExitSource_storagegroup_name(ctx *Source_storagegroup_nameContext) {}

// EnterTarget_storagegroup_name is called when production target_storagegroup_name is entered.
func (s *BaseDb2ParserListener) EnterTarget_storagegroup_name(ctx *Target_storagegroup_nameContext) {}

// ExitTarget_storagegroup_name is called when production target_storagegroup_name is exited.
func (s *BaseDb2ParserListener) ExitTarget_storagegroup_name(ctx *Target_storagegroup_nameContext) {}

// EnterSource_tablespace_name is called when production source_tablespace_name is entered.
func (s *BaseDb2ParserListener) EnterSource_tablespace_name(ctx *Source_tablespace_nameContext) {}

// ExitSource_tablespace_name is called when production source_tablespace_name is exited.
func (s *BaseDb2ParserListener) ExitSource_tablespace_name(ctx *Source_tablespace_nameContext) {}

// EnterTarget_tablespace_name is called when production target_tablespace_name is entered.
func (s *BaseDb2ParserListener) EnterTarget_tablespace_name(ctx *Target_tablespace_nameContext) {}

// ExitTarget_tablespace_name is called when production target_tablespace_name is exited.
func (s *BaseDb2ParserListener) ExitTarget_tablespace_name(ctx *Target_tablespace_nameContext) {}

// EnterUnqualified_function_name is called when production unqualified_function_name is entered.
func (s *BaseDb2ParserListener) EnterUnqualified_function_name(ctx *Unqualified_function_nameContext) {
}

// ExitUnqualified_function_name is called when production unqualified_function_name is exited.
func (s *BaseDb2ParserListener) ExitUnqualified_function_name(ctx *Unqualified_function_nameContext) {
}

// EnterUnqualified_procedure_name is called when production unqualified_procedure_name is entered.
func (s *BaseDb2ParserListener) EnterUnqualified_procedure_name(ctx *Unqualified_procedure_nameContext) {
}

// ExitUnqualified_procedure_name is called when production unqualified_procedure_name is exited.
func (s *BaseDb2ParserListener) ExitUnqualified_procedure_name(ctx *Unqualified_procedure_nameContext) {
}

// EnterUnqualified_specific_name is called when production unqualified_specific_name is entered.
func (s *BaseDb2ParserListener) EnterUnqualified_specific_name(ctx *Unqualified_specific_nameContext) {
}

// ExitUnqualified_specific_name is called when production unqualified_specific_name is exited.
func (s *BaseDb2ParserListener) ExitUnqualified_specific_name(ctx *Unqualified_specific_nameContext) {
}

// EnterPeriod_name is called when production period_name is entered.
func (s *BaseDb2ParserListener) EnterPeriod_name(ctx *Period_nameContext) {}

// ExitPeriod_name is called when production period_name is exited.
func (s *BaseDb2ParserListener) ExitPeriod_name(ctx *Period_nameContext) {}

// EnterHistory_table_name is called when production history_table_name is entered.
func (s *BaseDb2ParserListener) EnterHistory_table_name(ctx *History_table_nameContext) {}

// ExitHistory_table_name is called when production history_table_name is exited.
func (s *BaseDb2ParserListener) ExitHistory_table_name(ctx *History_table_nameContext) {}

// EnterXml_schema_name is called when production xml_schema_name is entered.
func (s *BaseDb2ParserListener) EnterXml_schema_name(ctx *Xml_schema_nameContext) {}

// ExitXml_schema_name is called when production xml_schema_name is exited.
func (s *BaseDb2ParserListener) ExitXml_schema_name(ctx *Xml_schema_nameContext) {}

// EnterTodo is called when production todo is entered.
func (s *BaseDb2ParserListener) EnterTodo(ctx *TodoContext) {}

// ExitTodo is called when production todo is exited.
func (s *BaseDb2ParserListener) ExitTodo(ctx *TodoContext) {}
