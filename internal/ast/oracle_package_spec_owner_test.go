package ast

import "testing"

func TestPackageSpecParametersBelongToTheirSubprogram(t *testing.T) {
	pf := plsqlFixture(t, "pck_cobranca.sql", `
CREATE EDITIONABLE PACKAGE PCK_COBRANCA
 AS
 PROCEDURE LISTAR_PENDENCIA
 (
   E_TX_ERRO   OUT VARCHAR2
  ,P_ID_LOTE IN  NUMBER
 );

 PROCEDURE ATUALIZAR_INTEGRACAO
 (
   E_TX_ERRO     OUT VARCHAR2
  ,P_LOG_TX IN  VARCHAR2
  ,P_STATUS    IN  VARCHAR2
 );

 FUNCTION SALDO_DEVEDOR
 (
   P_ID_CONTRATO IN NUMBER
 ) RETURN NUMBER;
END PCK_COBRANCA;
`)

	owners := map[string]Entity{}
	for _, p := range entitiesOfLabel(pf, "Parameter") {
		owners[p.Name] = p
	}

	want := map[string]struct{ context, contextType string }{
		"P_ID_LOTE":     {"LISTAR_PENDENCIA", "Procedure"},
		"P_LOG_TX":      {"ATUALIZAR_INTEGRACAO", "Procedure"},
		"P_STATUS":      {"ATUALIZAR_INTEGRACAO", "Procedure"},
		"P_ID_CONTRATO": {"SALDO_DEVEDOR", "Function"},
	}

	for name, exp := range want {
		got, ok := owners[name]
		if !ok {
			t.Errorf("%s was not extracted at all", name)
			continue
		}
		if got.Context != exp.context || got.ContextType != exp.contextType {
			t.Errorf("%s owned by %s %q, want %s %q",
				name, got.ContextType, got.Context, exp.contextType, exp.context)
		}
		if got.Context == "PCK_COBRANCA" {
			t.Errorf("%s fell back to the package — procedure_spec/function_spec "+
				"are missing from context_types", name)
		}
	}
}

// The same shape in a package BODY already worked, and has to keep working: the fix
// added contexts, it did not move the ones that were right.
func TestPackageBodyParametersStillBelongToTheirSubprogram(t *testing.T) {
	pf := plsqlFixture(t, "pck_cobranca_body.sql", `
CREATE EDITIONABLE PACKAGE BODY PCK_COBRANCA
 AS
 PROCEDURE ATUALIZAR_INTEGRACAO
 (
   E_TX_ERRO     OUT VARCHAR2
  ,P_LOG_TX IN  VARCHAR2
 )
 IS
 BEGIN
   NULL;
 END ATUALIZAR_INTEGRACAO;
END PCK_COBRANCA;
`)

	var found bool
	for _, p := range entitiesOfLabel(pf, "Parameter") {
		if p.Name != "P_LOG_TX" {
			continue
		}
		found = true
		if p.Context != "ATUALIZAR_INTEGRACAO" || p.ContextType != "Procedure" {
			t.Errorf("P_LOG_TX owned by %s %q, want Procedure %q",
				p.ContextType, p.Context, "ATUALIZAR_INTEGRACAO")
		}
	}
	if !found {
		t.Error("P_LOG_TX was not extracted from the package body")
	}
}

// A parameter with no owner is DISCARDED by ConvertToCache, so an undeclared context
// does not merely misfile it — it loses it. This asserts the cache keeps them, which
// is the same guarantee TestOracleParametersReachTheCache makes over a real corpus,
// without needing one.
func TestPackageSpecParametersSurviveIntoTheCache(t *testing.T) {
	proj := plsqlProject(t)
	pf := plsqlParse(t, proj, "pck_min.sql", `
CREATE PACKAGE PCK_MIN AS
 PROCEDURE P (P_A IN NUMBER, P_B IN NUMBER);
END PCK_MIN;
`)

	entry := ConvertToCache(pf, proj, true, "")
	if entry == nil {
		t.Fatal("ConvertToCache returned nothing")
	}

	params := map[string]bool{}
	for _, e := range entry.Entities {
		if e.Label == "Parameter" {
			params[e.Name] = true
		}
	}
	for _, want := range []string{"P_A", "P_B"} {
		if !params[want] {
			t.Errorf("%s did not reach the cache — an owner-less parameter is dropped", want)
		}
	}
}
