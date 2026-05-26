package ast

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

type configFileEntry struct {
	Filename  string
	Language  string
	Ecosystem string
}

var knownConfigFiles = []configFileEntry{

	{"tsconfig.json", "typescript", "node"},
	{"package.json", "javascript", "node"},
	{"jsconfig.json", "javascript", "node"},
	{".babelrc", "javascript", "babel"},
	{"babel.config.js", "javascript", "babel"},
	{"webpack.config.js", "javascript", "webpack"},
	{"webpack.config.ts", "typescript", "webpack"},
	{"vite.config.ts", "typescript", "vite"},
	{"vite.config.js", "javascript", "vite"},
	{"rollup.config.js", "javascript", "rollup"},
	{"esbuild.config.js", "javascript", "esbuild"},
	{"next.config.js", "javascript", "next"},
	{"next.config.mjs", "javascript", "next"},
	{"next.config.ts", "typescript", "next"},
	{"nuxt.config.ts", "typescript", "nuxt"},
	{"nuxt.config.js", "javascript", "nuxt"},
	{"svelte.config.js", "javascript", "svelte"},
	{"angular.json", "typescript", "angular"},
	{"remix.config.js", "javascript", "remix"},
	{"astro.config.mjs", "javascript", "astro"},
	{"turbo.json", "javascript", "turborepo"},
	{"nx.json", "typescript", "nx"},
	{"lerna.json", "javascript", "lerna"},
	{"pnpm-workspace.yaml", "javascript", "pnpm"},
	{".npmrc", "javascript", "npm"},
	{".yarnrc.yml", "javascript", "yarn"},
	{"eslint.config.js", "javascript", "eslint"},
	{".eslintrc.json", "javascript", "eslint"},
	{".prettierrc", "javascript", "prettier"},
	{"jest.config.js", "javascript", "jest"},
	{"jest.config.ts", "typescript", "jest"},
	{"vitest.config.ts", "typescript", "vitest"},
	{"playwright.config.ts", "typescript", "playwright"},
	{"cypress.config.ts", "typescript", "cypress"},
	{"tailwind.config.js", "javascript", "tailwind"},
	{"tailwind.config.ts", "typescript", "tailwind"},
	{"postcss.config.js", "javascript", "postcss"},

	{"go.mod", "go", "go"},
	{"go.sum", "go", "go"},
	{".golangci.yml", "go", "golangci"},
	{".golangci.yaml", "go", "golangci"},
	{"go.work", "go", "go_workspace"},

	{"pyproject.toml", "python", "python"},
	{"setup.py", "python", "setuptools"},
	{"setup.cfg", "python", "setuptools"},
	{"requirements.txt", "python", "pip"},
	{"Pipfile", "python", "pipenv"},
	{"poetry.lock", "python", "poetry"},
	{"tox.ini", "python", "tox"},
	{"mypy.ini", "python", "mypy"},
	{".flake8", "python", "flake8"},
	{"pytest.ini", "python", "pytest"},
	{".python-version", "python", "pyenv"},
	{"manage.py", "python", "django"},

	{"Cargo.toml", "rust", "cargo"},
	{"Cargo.lock", "rust", "cargo"},
	{"rust-toolchain.toml", "rust", "rustup"},
	{"rust-toolchain", "rust", "rustup"},
	{"clippy.toml", "rust", "clippy"},
	{".cargo/config.toml", "rust", "cargo_config"},

	{"pom.xml", "java", "maven"},
	{"build.gradle", "java", "gradle"},
	{"settings.gradle", "java", "gradle"},
	{"gradle.properties", "java", "gradle"},
	{"gradlew", "java", "gradle"},
	{"mvnw", "java", "maven"},
	{".mvn/wrapper/maven-wrapper.properties", "java", "maven"},
	{"checkstyle.xml", "java", "checkstyle"},
	{"spotbugs.xml", "java", "spotbugs"},

	{"build.gradle.kts", "kotlin", "gradle"},
	{"settings.gradle.kts", "kotlin", "gradle"},
	{"detekt.yml", "kotlin", "detekt"},

	{"*.csproj", "csharp", "dotnet"},
	{"*.sln", "csharp", "dotnet"},
	{"global.json", "csharp", "dotnet"},
	{"nuget.config", "csharp", "nuget"},
	{"Directory.Build.props", "csharp", "msbuild"},
	{"Directory.Build.targets", "csharp", "msbuild"},
	{".editorconfig", "csharp", "editorconfig"},

	{"composer.json", "php", "composer"},
	{"composer.lock", "php", "composer"},
	{"phpunit.xml", "php", "phpunit"},
	{"phpunit.xml.dist", "php", "phpunit"},
	{"phpstan.neon", "php", "phpstan"},
	{"phpstan.neon.dist", "php", "phpstan"},
	{".php-cs-fixer.php", "php", "php_cs_fixer"},
	{".php-cs-fixer.dist.php", "php", "php_cs_fixer"},
	{"psalm.xml", "php", "psalm"},
	{"artisan", "php", "laravel"},

	{"Gemfile", "ruby", "bundler"},
	{"Gemfile.lock", "ruby", "bundler"},
	{"Rakefile", "ruby", "rake"},
	{".rubocop.yml", "ruby", "rubocop"},
	{".ruby-version", "ruby", "rbenv"},
	{".rspec", "ruby", "rspec"},
	{"config.ru", "ruby", "rack"},

	{"Package.swift", "swift", "spm"},
	{"Package.resolved", "swift", "spm"},
	{"Podfile", "swift", "cocoapods"},
	{"Podfile.lock", "swift", "cocoapods"},
	{".swiftlint.yml", "swift", "swiftlint"},
	{"*.xcodeproj", "swift", "xcode"},
	{"*.xcworkspace", "swift", "xcode"},
	{"project.pbxproj", "swift", "xcode"},

	{"pubspec.yaml", "dart", "pub"},
	{"pubspec.lock", "dart", "pub"},
	{"analysis_options.yaml", "dart", "dart_analyzer"},
	{".flutter-plugins", "dart", "flutter"},
	{".metadata", "dart", "flutter"},

	{"CMakeLists.txt", "c", "cmake"},
	{"Makefile", "c", "make"},
	{"meson.build", "c", "meson"},
	{"conanfile.txt", "cpp", "conan"},
	{"conanfile.py", "cpp", "conan"},
	{"vcpkg.json", "cpp", "vcpkg"},
	{".clang-format", "cpp", "clang"},
	{".clang-tidy", "cpp", "clang"},
	{"compile_commands.json", "cpp", "clangd"},
	{"configure.ac", "c", "autotools"},
	{"configure", "c", "autotools"},
}

func DetectProjectConfig(ctx context.Context, db GraphDB, rootPath string) map[string]string {
	detected := make(map[string]string)

	for _, cfg := range knownConfigFiles {
		if strings.Contains(cfg.Filename, "*") {

			matches, _ := filepath.Glob(filepath.Join(rootPath, cfg.Filename))
			if len(matches) > 0 {
				detected[cfg.Ecosystem] = cfg.Language
			}
			continue
		}
		path := filepath.Join(rootPath, cfg.Filename)
		if _, err := os.Stat(path); err == nil {
			detected[cfg.Ecosystem] = cfg.Language
		}
	}

	goModPath := filepath.Join(rootPath, "go.mod")
	if data, err := os.ReadFile(goModPath); err == nil {
		for _, line := range strings.Split(string(data), "\n") {
			line = strings.TrimSpace(line)
			if strings.HasPrefix(line, "module ") {
				detected["go_module"] = strings.TrimPrefix(line, "module ")
				break
			}
		}
	}

	pkgPath := filepath.Join(rootPath, "package.json")
	if data, err := os.ReadFile(pkgPath); err == nil {
		var pkg struct {
			Name string `json:"name"`
			Main string `json:"main"`
		}
		if json.Unmarshal(data, &pkg) == nil && pkg.Name != "" {
			detected["npm_package"] = pkg.Name
			if pkg.Main != "" {
				detected["npm_main"] = pkg.Main
			}
		}
	}

	return detected
}

type frameworkRule struct {
	Decorator string
	Framework string
	Category  string
}

var frameworkRules = []frameworkRule{

	{"Controller", "spring", "web"},
	{"RestController", "spring", "web"},
	{"Service", "spring", "di"},
	{"Repository", "spring", "orm"},
	{"Component", "spring", "di"},
	{"Autowired", "spring", "di"},
	{"Inject", "spring", "di"},
	{"Bean", "spring", "di"},
	{"Configuration", "spring", "config"},
	{"Value", "spring", "config"},
	{"RequestMapping", "spring", "web"},
	{"GetMapping", "spring", "web"},
	{"PostMapping", "spring", "web"},
	{"PutMapping", "spring", "web"},
	{"DeleteMapping", "spring", "web"},
	{"PatchMapping", "spring", "web"},
	{"PathVariable", "spring", "web"},
	{"RequestBody", "spring", "web"},
	{"RequestParam", "spring", "web"},
	{"ResponseBody", "spring", "web"},
	{"ResponseStatus", "spring", "web"},
	{"ExceptionHandler", "spring", "web"},
	{"CrossOrigin", "spring", "web"},
	{"Validated", "spring", "validation"},
	{"Valid", "spring", "validation"},
	{"Transactional", "spring", "orm"},
	{"EnableAutoConfiguration", "spring_boot", "config"},
	{"SpringBootApplication", "spring_boot", "config"},
	{"SpringBootTest", "spring_boot", "test"},

	{"Entity", "jpa", "orm"},
	{"Table", "jpa", "orm"},
	{"Column", "jpa", "orm"},
	{"Id", "jpa", "orm"},
	{"GeneratedValue", "jpa", "orm"},
	{"ManyToOne", "jpa", "orm"},
	{"OneToMany", "jpa", "orm"},
	{"ManyToMany", "jpa", "orm"},
	{"OneToOne", "jpa", "orm"},
	{"JoinColumn", "jpa", "orm"},
	{"Query", "jpa", "orm"},

	{"Test", "junit", "test"},
	{"BeforeEach", "junit", "test"},
	{"AfterEach", "junit", "test"},
	{"BeforeAll", "junit", "test"},
	{"AfterAll", "junit", "test"},
	{"DisplayName", "junit", "test"},
	{"ParameterizedTest", "junit", "test"},
	{"MockBean", "mockito", "test"},
	{"Mock", "mockito", "test"},
	{"InjectMocks", "mockito", "test"},

	{"Stateless", "jakarta", "ejb"},
	{"Stateful", "jakarta", "ejb"},
	{"Singleton", "jakarta", "ejb"},
	{"Path", "jaxrs", "web"},
	{"GET", "jaxrs", "web"},
	{"POST", "jaxrs", "web"},
	{"PUT", "jaxrs", "web"},
	{"DELETE", "jaxrs", "web"},
	{"Produces", "jaxrs", "web"},
	{"Consumes", "jaxrs", "web"},

	{"Composable", "jetpack_compose", "ui"},
	{"Preview", "jetpack_compose", "ui"},
	{"HiltViewModel", "hilt", "di"},
	{"HiltAndroidApp", "hilt", "di"},
	{"AndroidEntryPoint", "hilt", "di"},
	{"Parcelize", "android", "serialization"},
	{"SerializedName", "gson", "serialization"},

	{"Serializable", "kotlinx", "serialization"},

	{"ApiController", "aspnet", "web"},
	{"HttpGet", "aspnet", "web"},
	{"HttpPost", "aspnet", "web"},
	{"HttpPut", "aspnet", "web"},
	{"HttpDelete", "aspnet", "web"},
	{"HttpPatch", "aspnet", "web"},
	{"Authorize", "aspnet", "auth"},
	{"AllowAnonymous", "aspnet", "auth"},
	{"Route", "aspnet", "web"},
	{"FromBody", "aspnet", "web"},
	{"FromQuery", "aspnet", "web"},
	{"FromRoute", "aspnet", "web"},
	{"ProducesResponseType", "aspnet", "web"},
	{"ValidateAntiForgeryToken", "aspnet", "web"},
	{"BindProperty", "aspnet_razor", "web"},

	{"DbContext", "ef_core", "orm"},
	{"DbSet", "ef_core", "orm"},
	{"Key", "ef_core", "orm"},
	{"Required", "ef_core", "validation"},
	{"MaxLength", "ef_core", "validation"},
	{"ForeignKey", "ef_core", "orm"},
	{"NotMapped", "ef_core", "orm"},

	{"Parameter", "blazor", "ui"},
	{"CascadingParameter", "blazor", "ui"},
	{"Inject", "blazor", "di"},
	{"JSInvokable", "blazor", "interop"},
	{"HubMethodName", "signalr", "realtime"},

	{"Fact", "xunit", "test"},
	{"Theory", "xunit", "test"},
	{"InlineData", "xunit", "test"},
	{"ClassData", "xunit", "test"},
	{"TestFixture", "nunit", "test"},
	{"SetUp", "nunit", "test"},
	{"TearDown", "nunit", "test"},

	{"app.get", "fastapi", "web"},
	{"app.post", "fastapi", "web"},
	{"app.put", "fastapi", "web"},
	{"app.delete", "fastapi", "web"},
	{"app.patch", "fastapi", "web"},
	{"app.middleware", "fastapi", "middleware"},
	{"router.get", "fastapi", "web"},
	{"router.post", "fastapi", "web"},
	{"Depends", "fastapi", "di"},
	{"route", "flask", "web"},
	{"before_request", "flask", "middleware"},
	{"login_required", "django", "auth"},
	{"permission_required", "django", "auth"},
	{"csrf_exempt", "django", "web"},
	{"require_http_methods", "django", "web"},
	{"admin.register", "django", "admin"},
	{"receiver", "django", "signals"},

	{"declared_attr", "sqlalchemy", "orm"},
	{"validates", "sqlalchemy", "orm"},
	{"hybrid_property", "sqlalchemy", "orm"},

	{"task", "celery", "async"},
	{"shared_task", "celery", "async"},
	{"periodic_task", "celery", "async"},

	{"pytest.fixture", "pytest", "test"},
	{"pytest.mark.parametrize", "pytest", "test"},
	{"pytest.mark.skip", "pytest", "test"},
	{"pytest.mark.asyncio", "pytest", "test"},
	{"patch", "unittest", "test"},

	{"validator", "pydantic", "validation"},
	{"field_validator", "pydantic", "validation"},
	{"model_validator", "pydantic", "validation"},

	{"Controller", "nestjs", "web"},
	{"Injectable", "nestjs", "di"},
	{"Module", "nestjs", "di"},
	{"Get", "nestjs", "web"},
	{"Post", "nestjs", "web"},
	{"Put", "nestjs", "web"},
	{"Delete", "nestjs", "web"},
	{"Patch", "nestjs", "web"},
	{"Body", "nestjs", "web"},
	{"Param", "nestjs", "web"},
	{"Query", "nestjs", "web"},
	{"Guard", "nestjs", "auth"},
	{"UseGuards", "nestjs", "auth"},
	{"UseInterceptors", "nestjs", "middleware"},
	{"UsePipes", "nestjs", "validation"},
	{"Cron", "nestjs", "scheduling"},

	{"middleware", "express", "middleware"},

	{"Component", "angular", "ui"},
	{"NgModule", "angular", "di"},
	{"Input", "angular", "ui"},
	{"Output", "angular", "ui"},
	{"ViewChild", "angular", "ui"},
	{"HostListener", "angular", "ui"},
	{"Pipe", "angular", "ui"},
	{"Directive", "angular", "ui"},

	{"Options", "vue", "ui"},
	{"Prop", "vue", "ui"},
	{"Watch", "vue", "ui"},
	{"Emit", "vue", "ui"},

	{"PrimaryGeneratedColumn", "typeorm", "orm"},
	{"ManyToOne", "typeorm", "orm"},
	{"OneToMany", "typeorm", "orm"},
	{"JoinColumn", "typeorm", "orm"},

	{"Meta", "storybook", "test"},
	{"Story", "storybook", "test"},

	{"middleware", "laravel", "middleware"},

	{"Route", "symfony", "web"},
	{"IsGranted", "symfony", "auth"},
	{"ParamConverter", "symfony", "web"},
	{"Template", "symfony", "web"},
	{"Security", "symfony", "auth"},
	{"Assert\\NotBlank", "symfony", "validation"},
	{"Assert\\Valid", "symfony", "validation"},
	{"ORM\\Entity", "doctrine", "orm"},
	{"ORM\\Table", "doctrine", "orm"},
	{"ORM\\Column", "doctrine", "orm"},
	{"ORM\\Id", "doctrine", "orm"},
	{"ORM\\ManyToOne", "doctrine", "orm"},
	{"ORM\\OneToMany", "doctrine", "orm"},

	{"test", "phpunit", "test"},
	{"dataProvider", "phpunit", "test"},
	{"depends", "phpunit", "test"},
	{"covers", "phpunit", "test"},

	{"before_action", "rails", "web"},
	{"after_action", "rails", "web"},
	{"around_action", "rails", "web"},
	{"skip_before_action", "rails", "web"},
	{"validates", "rails", "validation"},
	{"has_many", "rails", "orm"},
	{"belongs_to", "rails", "orm"},
	{"has_one", "rails", "orm"},
	{"has_and_belongs_to_many", "rails", "orm"},
	{"scope", "rails", "orm"},

	{"describe", "rspec", "test"},
	{"context", "rspec", "test"},
	{"it", "rspec", "test"},
	{"subject", "rspec", "test"},
	{"let", "rspec", "test"},

	{"get", "sinatra", "web"},
	{"post", "sinatra", "web"},
	{"put", "sinatra", "web"},
	{"delete", "sinatra", "web"},

	{"actix_web", "actix", "web"},
	{"get", "actix", "web"},
	{"post", "actix", "web"},
	{"rocket::get", "rocket", "web"},
	{"rocket::post", "rocket", "web"},
	{"rocket::launch", "rocket", "web"},
	{"tokio::main", "tokio", "async"},
	{"tokio::test", "tokio", "test"},
	{"async_trait", "async_trait", "async"},

	{"derive", "serde", "serialization"},
	{"Serialize", "serde", "serialization"},
	{"Deserialize", "serde", "serialization"},
	{"Insertable", "diesel", "orm"},
	{"Queryable", "diesel", "orm"},
	{"AsChangeset", "diesel", "orm"},
	{"table", "diesel", "orm"},

	{"test", "rust_test", "test"},
	{"cfg(test)", "rust_test", "test"},
	{"should_panic", "rust_test", "test"},

	{"State", "swiftui", "ui"},
	{"Binding", "swiftui", "ui"},
	{"Published", "swiftui", "ui"},
	{"ObservedObject", "swiftui", "ui"},
	{"EnvironmentObject", "swiftui", "ui"},
	{"StateObject", "swiftui", "ui"},
	{"Environment", "swiftui", "ui"},
	{"ViewBuilder", "swiftui", "ui"},
	{"AppStorage", "swiftui", "ui"},
	{"FetchRequest", "swiftui", "data"},

	{"RouteCollection", "vapor", "web"},

	{"objc", "uikit", "ui"},
	{"IBAction", "uikit", "ui"},
	{"IBOutlet", "uikit", "ui"},
	{"available", "swift", "compat"},
	{"discardableResult", "swift", "core"},
	{"testable", "xctest", "test"},

	{"override", "dart", "core"},
	{"required", "dart", "core"},
	{"visibleForTesting", "flutter", "test"},
	{"immutable", "flutter", "core"},
	{"protected", "flutter", "core"},
	{"mustCallSuper", "flutter", "core"},
	{"optionalTypeArgs", "flutter", "core"},
}

func DetectFrameworks(ctx context.Context, db GraphDB) map[string]string {
	detected := make(map[string]string)

	for _, label := range []string{"Function", "Method", "Class"} {
		q := fmt.Sprintf(`MATCH (n:%s) WHERE n.decorators IS NOT NULL AND len(n.decorators) > 0 RETURN n.decorators AS decorators LIMIT 1000`, label)
		res, err := db.Query(ctx, q, nil)
		if err != nil {
			continue
		}
		for _, rec := range res.Records {
			switch v := rec["decorators"].(type) {
			case []any:
				for _, d := range v {
					if s, ok := d.(string); ok && s != "" {
						matchSingleDecorator(s, detected)
					}
				}
			case string:
				if v != "" {
					matchDecorators(v, detected)
				}
			}
		}
	}

	heritageRules := map[string][2]string{

		"Model":               {"django", "orm"},
		"ModelForm":           {"django", "forms"},
		"APIView":             {"django_rest", "web"},
		"ViewSet":             {"django_rest", "web"},
		"ModelViewSet":        {"django_rest", "web"},
		"GenericAPIView":      {"django_rest", "web"},
		"TestCase":            {"django", "test"},
		"SimpleTestCase":      {"django", "test"},
		"TransactionTestCase": {"django", "test"},

		"FlaskForm":  {"flask_wtf", "forms"},
		"Resource":   {"flask_restful", "web"},
		"MethodView": {"flask", "web"},

		"ApplicationRecord":      {"rails", "orm"},
		"ActiveRecord::Base":     {"rails", "orm"},
		"ApplicationController":  {"rails", "web"},
		"ActionController::Base": {"rails", "web"},
		"ApplicationMailer":      {"rails", "mail"},
		"ActiveJob::Base":        {"rails", "async"},

		"Component":     {"react", "ui"},
		"PureComponent": {"react", "ui"},

		"OnInit":    {"angular", "ui"},
		"OnDestroy": {"angular", "ui"},

		"StatelessWidget": {"flutter", "ui"},
		"StatefulWidget":  {"flutter", "ui"},
		"State":           {"flutter", "ui"},
		"ChangeNotifier":  {"flutter", "state"},
		"InheritedWidget": {"flutter", "ui"},
		"Cubit":           {"flutter_bloc", "state"},
		"Bloc":            {"flutter_bloc", "state"},

		"ControllerBase": {"aspnet", "web"},
		"Controller":     {"aspnet", "web"},
		"PageModel":      {"aspnet_razor", "web"},
		"Hub":            {"signalr", "realtime"},
		"DbContext":      {"ef_core", "orm"},

		"JpaRepository":  {"spring_data", "orm"},
		"CrudRepository": {"spring_data", "orm"},

		"Activity":          {"android", "ui"},
		"AppCompatActivity": {"android", "ui"},
		"Fragment":          {"android", "ui"},
		"ViewModel":         {"android", "arch"},
		"Application":       {"android", "core"},
		"Service":           {"android", "service"},
		"BroadcastReceiver": {"android", "service"},

		"UIViewController": {"uikit", "ui"},
		"UIView":           {"uikit", "ui"},
		"UITableViewCell":  {"uikit", "ui"},
		"ObservableObject": {"combine", "state"},
		"XCTestCase":       {"xctest", "test"},

		"Handler":     {"actix", "web"},
		"Responder":   {"actix", "web"},
		"FromRequest": {"actix", "web"},
	}

	q := `MATCH (c)-[:INHERITS]->(p) RETURN p.name AS parent LIMIT 500`
	if res, err := db.Query(ctx, q, nil); err == nil {
		for _, rec := range res.Records {
			parent, _ := rec["parent"].(string)
			if rule, ok := heritageRules[parent]; ok {
				detected[rule[0]] = rule[1]
			}
		}
	}

	q2 := `MATCH (c)-[:IMPLEMENTS]->(p) RETURN p.name AS parent LIMIT 500`
	if res, err := db.Query(ctx, q2, nil); err == nil {
		for _, rec := range res.Records {
			parent, _ := rec["parent"].(string)
			if rule, ok := heritageRules[parent]; ok {
				detected[rule[0]] = rule[1]
			}
		}
	}

	importRules := map[string][2]string{

		"github.com/gin-gonic/gin":    {"gin", "web"},
		"github.com/labstack/echo":    {"echo", "web"},
		"github.com/gofiber/fiber":    {"fiber", "web"},
		"github.com/gorilla/mux":      {"gorilla", "web"},
		"github.com/go-chi/chi":       {"chi", "web"},
		"net/http":                    {"stdlib", "web"},
		"google.golang.org/grpc":      {"grpc", "rpc"},
		"google.golang.org/protobuf":  {"protobuf", "serialization"},
		"gorm.io/gorm":                {"gorm", "orm"},
		"github.com/jmoiron/sqlx":     {"sqlx", "orm"},
		"go.uber.org/zap":             {"zap", "logging"},
		"github.com/stretchr/testify": {"testify", "test"},
		"github.com/spf13/cobra":      {"cobra", "cli"},
		"github.com/spf13/viper":      {"viper", "config"},

		"express":        {"express", "web"},
		"koa":            {"koa", "web"},
		"fastify":        {"fastify", "web"},
		"hono":           {"hono", "web"},
		"react":          {"react", "ui"},
		"react-dom":      {"react", "ui"},
		"next":           {"next", "web"},
		"vue":            {"vue", "ui"},
		"nuxt":           {"nuxt", "web"},
		"svelte":         {"svelte", "ui"},
		"@angular/core":  {"angular", "ui"},
		"@nestjs/common": {"nestjs", "web"},
		"prisma":         {"prisma", "orm"},
		"typeorm":        {"typeorm", "orm"},
		"sequelize":      {"sequelize", "orm"},
		"mongoose":       {"mongoose", "orm"},
		"graphql":        {"graphql", "api"},
		"apollo":         {"apollo", "api"},
		"trpc":           {"trpc", "api"},
		"socket.io":      {"socketio", "realtime"},
		"jest":           {"jest", "test"},
		"mocha":          {"mocha", "test"},
		"vitest":         {"vitest", "test"},
		"cypress":        {"cypress", "test"},
		"playwright":     {"playwright", "test"},
		"tailwindcss":    {"tailwind", "css"},
		"redux":          {"redux", "state"},
		"zustand":        {"zustand", "state"},
		"mobx":           {"mobx", "state"},

		"django":     {"django", "web"},
		"flask":      {"flask", "web"},
		"fastapi":    {"fastapi", "web"},
		"sqlalchemy": {"sqlalchemy", "orm"},
		"celery":     {"celery", "async"},
		"pytest":     {"pytest", "test"},
		"pydantic":   {"pydantic", "validation"},
		"numpy":      {"numpy", "data"},
		"pandas":     {"pandas", "data"},
		"torch":      {"pytorch", "ml"},
		"tensorflow": {"tensorflow", "ml"},
		"scrapy":     {"scrapy", "scraping"},

		"Illuminate": {"laravel", "web"},
		"Symfony":    {"symfony", "web"},
		"Doctrine":   {"doctrine", "orm"},

		"rails":   {"rails", "web"},
		"sinatra": {"sinatra", "web"},
		"rspec":   {"rspec", "test"},
		"sidekiq": {"sidekiq", "async"},
	}

	q3 := `MATCH (f:File)-[:IMPORTS]->(m:Module) RETURN m.name AS name, m.full_import_name AS full_name LIMIT 1000`
	if res, err := db.Query(ctx, q3, nil); err == nil {
		for _, rec := range res.Records {
			name, _ := rec["name"].(string)
			fullName, _ := rec["full_name"].(string)

			for pattern, rule := range importRules {
				if fullName == pattern || strings.HasPrefix(fullName, pattern+"/") ||
					name == pattern || strings.HasPrefix(name, pattern) {
					detected[rule[0]] = rule[1]
				}
			}
		}
	}

	return detected
}

func matchDecorators(decStr string, detected map[string]string) {
	for _, dec := range strings.Split(decStr, ",") {
		dec = strings.TrimSpace(dec)
		matchSingleDecorator(dec, detected)
	}
}

func matchSingleDecorator(dec string, detected map[string]string) {
	if dec == "" {
		return
	}
	for _, rule := range frameworkRules {
		if dec == rule.Decorator || strings.HasSuffix(dec, "."+rule.Decorator) ||
			strings.HasSuffix(dec, "\\"+rule.Decorator) {
			detected[rule.Framework] = rule.Category
		}
	}
}

func ScoreEntryPoints(ctx context.Context, db GraphDB) {

	q := `MATCH (f:Function) RETURN f.uid AS uid, f.name AS name, f.decorators AS decorators, f.is_exported AS is_exported, f.lang AS lang LIMIT 5000`
	res, err := db.Query(ctx, q, nil)
	if err != nil {
		return
	}

	for _, rec := range res.Records {
		uid, _ := rec["uid"].(string)
		name, _ := rec["name"].(string)
		isExported, _ := rec["is_exported"].(bool)
		lang, _ := rec["lang"].(string)

		var decorators string
		switch v := rec["decorators"].(type) {
		case []any:
			parts := make([]string, 0, len(v))
			for _, d := range v {
				if s, ok := d.(string); ok {
					parts = append(parts, s)
				}
			}
			decorators = strings.Join(parts, ",")
		case string:
			decorators = v
		}

		if uid == "" || name == "" {
			continue
		}

		score := scoreFunction(name, decorators, isExported, lang)
		if score == 0 {
			continue
		}

		uq := `MATCH (f:Function {uid: $uid}) SET f.entry_point_score = $score`
		db.Execute(ctx, uq, map[string]any{"uid": uid, "score": score})
	}
}

func scoreFunction(name, decorators string, isExported bool, lang string) int {
	score := 0

	nameLower := strings.ToLower(name)

	if name == "main" || name == "Main" {
		score += 80
	}

	if strings.HasPrefix(name, "Test") || strings.HasPrefix(name, "test_") ||
		strings.HasPrefix(nameLower, "test") {
		score += 60
	}

	if name == "init" || name == "setup" || name == "configure" {
		score += 40
	}

	if strings.HasSuffix(nameLower, "handler") || strings.HasSuffix(nameLower, "controller") {
		score += 30
	}

	if strings.HasPrefix(nameLower, "cmd") || strings.HasPrefix(nameLower, "run") {
		score += 20
	}

	if isExported {
		score += 10
	}

	if decorators != "" {
		for _, dec := range strings.Split(decorators, ",") {
			dec = strings.TrimSpace(dec)
			decLower := strings.ToLower(dec)

			if dec == "GetMapping" || dec == "PostMapping" || dec == "PutMapping" || dec == "DeleteMapping" ||
				dec == "RequestMapping" || dec == "HttpGet" || dec == "HttpPost" || dec == "HttpPut" || dec == "HttpDelete" ||
				dec == "Get" || dec == "Post" || dec == "Put" || dec == "Delete" ||
				dec == "Route" || dec == "route" {
				score += 70
			}

			if strings.HasPrefix(decLower, "app.") && (strings.HasSuffix(decLower, "get") ||
				strings.HasSuffix(decLower, "post") || strings.HasSuffix(decLower, "put") ||
				strings.HasSuffix(decLower, "delete")) {
				score += 70
			}

			if dec == "Test" || dec == "test" || dec == "pytest.fixture" {
				score += 60
			}

			if dec == "Controller" || dec == "RestController" || dec == "ApiController" {
				score += 50
			}
		}
	}

	switch lang {
	case "go":

		if name == "init" {
			score = max(score, 70)
		}
	case "python":

		if name == "__main__" {
			score = max(score, 80)
		}
	}

	if score > 100 {
		score = 100
	}

	return score
}

func RunEnrichment(ctx context.Context, db GraphDB, rootPath string) {

	configs := DetectProjectConfig(ctx, db, rootPath)
	if len(configs) > 0 {

		props := make([]string, 0, len(configs))
		for k, v := range configs {
			props = append(props, k+"="+v)
		}
		configStr := strings.Join(props, ",")
		q := `MERGE (c:File {path: '__config__'}) SET c.name = 'project_config', c.source = $config`
		db.Execute(ctx, q, map[string]any{"config": configStr})
	}

	frameworks := DetectFrameworks(ctx, db)
	if len(frameworks) > 0 {
		fwParts := make([]string, 0, len(frameworks))
		for fw, cat := range frameworks {
			fwParts = append(fwParts, fw+":"+cat)
		}
		fwStr := strings.Join(fwParts, ",")
		q := `MERGE (c:File {path: '__config__'}) SET c.lang = $frameworks`
		db.Execute(ctx, q, map[string]any{"frameworks": fwStr})
	}

	ScoreEntryPoints(ctx, db)

	if len(configs) > 0 || len(frameworks) > 0 {
		fmt.Fprintf(os.Stderr, "  › Enrichment: configs=%d frameworks=%d\n", len(configs), len(frameworks))
	}
}
