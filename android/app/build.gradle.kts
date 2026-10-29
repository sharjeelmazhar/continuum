plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "io.github.sharjeelmazhar.continuum"
    compileSdk = 37

    defaultConfig {
        applicationId = "io.github.sharjeelmazhar.continuum"
        minSdk = 26
        targetSdk = 37
        // The release workflow passes these from the git tag (v1.2.3 -> 1.2.3 / 10203).
        versionCode = providers.gradleProperty("versionCode").orNull?.toInt() ?: 1
        versionName = providers.gradleProperty("versionName").orNull ?: "0.1.0"
    }

    // Release signing key comes from the environment (GitHub secrets in CI), never from the repo.
    val keystore = providers.environmentVariable("CONTINUUM_KEYSTORE").orNull
    signingConfigs {
        if (keystore != null) {
            create("release") {
                storeFile = file(keystore)
                storePassword = providers.environmentVariable("CONTINUUM_KEYSTORE_PASSWORD").get()
                keyAlias = providers.environmentVariable("CONTINUUM_KEY_ALIAS").get()
                keyPassword = providers.environmentVariable("CONTINUUM_KEY_PASSWORD").get()
            }
        }
    }

    buildTypes {
        release {
            if (keystore != null) signingConfig = signingConfigs.getByName("release")
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }

    lint {
        warningsAsErrors = false
        abortOnError = true
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    debugImplementation(libs.compose.ui.tooling)

    testImplementation(libs.junit)
}
